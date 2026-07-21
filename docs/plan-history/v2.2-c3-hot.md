# plan-hot — v2.2 到 C3:两份 androidTest 各自进它该进的那道闸门

## 目标 checkpoint

C3:`android-gate-scan.py` 的 `DEFERRED` 表**不复存在**,因为两份 `src/androidTest/`
都已按各自的**真实种类**被闸门覆盖,而种类由脚本从磁盘证明、不由谁的记忆声明:

- **`:app`(runner 本体)** → 闸门形态是 **`assembleDebugAndroidTest`**,与 iOS 侧
  `xcodebuild build-for-testing -destination 'generic/platform=iOS Simulator'` **严格对称**:
  编译"发给用户、真正驱动设备的那个主体",**不起设备、不 instrument、不起 server**。
  它不可能挂,因为它压根不执行任何测试。
- **`:sdk`(断言套件)** → 在 **ship 闸门**里于钉住的 emulator 上真跑
  `connectedDebugAndroidTest`,判据不是 `failures=0`,而是**执行数等于磁盘上的 `@Test` 数**。
- **CI 与 preflight 只到编译层**,这是一个**明写的降级**:它被印在
  `android-gate-scan` 每次运行的输出行里,任何人看见 CI 绿都无法把它读成
  "Android 行为被覆盖了"。

C2 交下来的硬约束原样承接,不再讨论:**`:app:connectedDebugAndroidTest` 不进任何闸门**
(实测 3 分 40 秒停在 `Tests 0/1 completed` 而端口在服务 —— 它不会失败,它永不返回)。

本段热化前的本机实测,后面的步骤按它写:

- **裸任务名 `assembleDebugAndroidTest` 解析到两个模块**:
  `./gradlew assembleDebugAndroidTest --dry-run` 的任务图里
  `:app:assembleDebugAndroidTest` 与 `:sdk:assembleDebugAndroidTest` 同时出现
  (`:app:` 前缀任务 59 个)。所以三处闸门沿用 C1 的裸任务名写法,新模块到达即在闸门内。
- **`:app` 的 androidTest 里只有 1 个 `@Test`**(`RunnerTest.kt:42`,
  `dev.smix.runner.RunnerTest#runServerForever`),而
  `crates/smix-cli/src/runner_android.rs` 的 `SERVER_ENTRY` 常量正是这一串。
  两边任何一侧改名,`am instrument` 就会报 **`OK (0 tests)`** —— 该文件自己的注释写着
  这是"一个读起来像成功的静默 no-op"。**没有任何东西在守这个跨语言坐标**,S3 补上。
- **`:sdk` 的 androidTest 里有 4 个 `@Test`**(`Spike001Test` 3 + `ConformanceHarnessTest` 1),
  与 C2 实测的 `tests="4"` 一致。
- 本机 **没有 `timeout` / `gtimeout`**(C2 实测),所以 S2 的截止时间要自己实现;
  顺带发现 `scripts/release/corpus-gate.sh:116` 依赖 `timeout` —— 记账,不在本段修。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix

# 1) 护栏与源码闸门就位
bash scripts/dev/adb-guard.test.sh          # 期望:all cases pass
python3 scripts/dev/android-gate-scan.py    # 期望:android-gate-scan: clean(DEFERRED 两条仍在)
python3 scripts/dev/workflow-scan.py        # 期望:workflow-scan: clean

# 2) 没人在用 Android 工具链
pgrep -fl 'emulator|mobilegate'                             # 期望:空
( cd android-runner && ./gradlew --status --console=plain )  # 期望:无 BUSY daemon

# 3) 看清楚现在连着什么
adb devices
```

热化时第 1/2/3 条已实测:scan clean;无 emulator 与 mobilegate;两个 **IDLE** 9.3.1 daemon
(IDLE 是 wrapper 的正常产物,**不是**"有人在跑",别用 `pgrep gradle` 判占用);
`adb devices` 列着 `R5CT52DF07D` 与它的无线镜像 —— **物理手机在场是事实,不是风险**。
本段每一条钉 emulator 的写法都是承重的。

`ANDROID_SERIAL=emulator-5554` 必须写成**命令行内联前缀**:adb-guard 判的是这一条命令的
文本(`scripts/dev/adb-guard.sh:73-77`),`export` 在另一条命令里它看不见。被拦时
**改命令,不改 guard**。

起 emulator(本段唯一长驻进程,按 pid-registry 落 handle 再进 S1):

```bash
"$ANDROID_HOME/emulator/emulator" -avd sim-smix-android-01 -port 5554 -no-snapshot-save &
adb -s emulator-5554 wait-for-device
adb -s emulator-5554 shell getprop sys.boot_completed   # 期望输出 1
```

## 步骤(线性,无分叉)

### S1. 两份 androidTest 的**编译**进 preflight / CI / ship,与 iOS 的 build-for-testing 对称

`:app` 那 1241 行是**发给用户、真正驱动设备的 runner 本体**,而它**从来没有被任何闸门
编译过** —— `testDebugUnitTest` 不碰 androidTest source set。这与 iOS 侧记过两次的
「ship gate 从不编译分发给用户的运行器主体」是同一个物种,对策也该是同一个:
**编译它,不运行它**。

**红(证明现在的闸门放行一个编译不过的 runner 本体)**

```bash
cd /Users/doracawl/workspace/goliajp/smix
BAK=/tmp/smix-c3-runnertest.kt.bak
cp android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt "$BAK"
# 注入:在 RunnerTest 类体内加一行引用不存在符号的语句,使该 source set 编译失败
```

- **接线前**:`bash scripts/dev/preflight.sh; echo "exit=$?"` → 期望 **exit=0**
  (缺口证实:runner 本体编译不过,本地全套闸门全绿放行)
- **接线后**:`bash scripts/dev/preflight.sh; echo "exit=$?"` → 期望**非零**,
  输出点名 `RunnerTest.kt` 与编译错误
- **还原**:`cp "$BAK" android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt`
  **禁止 `git checkout <file>` 还原**(07-21 的过程失误:整体未提交的文件被一并抹掉)
- **还原后**:`bash scripts/dev/preflight.sh` → exit 0,末行 `preflight: clean`

**绿(接线,三处)**

三处都改成**一次 gradle 调用带两个裸任务名**:
`./gradlew testDebugUnitTest assembleDebugAndroidTest --console=plain`。
裸名的依据是热化实测的 `--dry-run` 任务图(两个模块都解析到),与 C1 同一条理由:
新模块到达即在闸门内。

- 文件:`scripts/dev/preflight.sh`(第 58-68 行的 `--- android unit tests` 段)
  - 段名改为 `--- android: unit tests + androidTest compile`
  - 命令加 `assembleDebugAndroidTest`
  - 段内注释补一句**为什么只到编译层**:instrumentation 需要设备,preflight 是每天
    跑几十次的本地习惯,起 emulator 会跟用户自己的工作抢机器;设备层在 ship.sh
    (S2)。**这句必须写,因为它是一次降级,降级要留名。**
- 文件:`.github/workflows/ci.yml`
  - job `android-unit` → 更名 **`android-no-device`**。理由与 C1 把 `kotlin-sdk` 改成
    `android-unit` 同源:名字说 `unit` 会让人以为 Android 只有单测这一层,而这个 job
    此刻覆盖的是"**所有不需要设备的 Android 检查**"—— 名字直接说出边界,
    "CI 绿 = Android 被覆盖"的错觉就无处生根
  - step 名 `unit tests + androidTest compile (no device — instrumentation runs at ship)`
  - run 改为 `./gradlew testDebugUnitTest assembleDebugAndroidTest --console=plain`
- 文件:`scripts/release/ship.sh`(第 83-96 行 `--- Android unit tests` 段)
  - 段名改为 `--- Android unit tests + androidTest compile`
  - 命令加 `assembleDebugAndroidTest`
  - 注释补两句:(1) `assembleDebugAndroidTest` 是 iOS 侧
    `xcodebuild build-for-testing`(本文件第 57-66 行)的 Android 对等物 —— 编译运行器
    主体而不启动设备;(2) `:app` 的 `connectedDebugAndroidTest` **不在这里也不会在这里**,
    C2 实测它 3 分 40 秒停在 `Tests 0/1 completed` 而 `/health` 返回 200,
    **它不会失败,它永不返回**

**重构**

- ship.sh 该段原有注释(bindings 首次编译发生在 publish 期 / 裸任务名的由来)仍然成立,
  保留;新增两句不覆盖它。
- 不动 iOS 侧那两段。对称是**形态**上的对称,不是把两边合并成一个循环(§8.1)。

### S2. `:sdk` 的断言套件进 ship 闸门:钉 emulator 跑,并且判据不是 `failures=0`

C2 已证明 `:sdk` 是真断言且首跑全绿。本步把它接进**发布路径**,并解决两个 C2 留下的
判据问题:`failures="0"` 对"一个测试都没跑"同样成立;而 `tests="4"` 是写死的数字,
将来加了第 5 条断言它就变成"少跑一条也算过"。

**红(三条注入,逐条,不合并)**

判据先于闸门:先写 `scripts/dev/androidtest-xml-judge.py`,它只做一件事 ——
比对"磁盘上的 `@Test` 数"与"结果 XML 里真跑了多少",**不碰设备**,于是它的红向验证
用手写 XML 就能做,不必反复起 emulator。

```bash
cd /Users/doracawl/workspace/goliajp/smix
mkdir -p /tmp/smix-c3-xml
```

1. **跑了 0 个却报绿**:手写一份 `tests="0" failures="0" errors="0" skipped="0"` 的
   `TEST-fake.xml` → `python3 scripts/dev/androidtest-xml-judge.py --module android-runner/sdk
   --results /tmp/smix-c3-xml; echo "exit=$?"` → 期望**非零**,消息说清
   "磁盘上有 4 个 `@Test`,结果里执行了 0 个"
2. **少跑了一条**:同上改 `tests="3"` → 期望**非零**,消息给出 4 与 3 两个数
3. **靠 `@Ignore` 变绿**:`tests="4" skipped="1"` → 期望**非零**,消息点名
   "跳过不是通过"(C2 明令禁止用 `@Ignore` 换绿,这条把禁令变成机器判据)
4. **结果目录整个不存在**:指向一个空目录 → 期望**非零**,消息区分
   "没有结果文件"与"结果文件说 0 个"—— 前者是任务根本没跑

再对**闸门脚本本身**注入两条,这两条需要 emulator:

5. **真断言失败**:备份后把 `Spike001Test` 首条断言的期望值改成必然不成立的字面量 →
   `SMIX_ANDROID_SERIAL=emulator-5554 bash scripts/release/android-instrumentation-gate.sh;
   echo "exit=$?"` → 期望**非零**,输出点名该测试全名。还原走备份文件,不走 git。
6. **设备选择自己必须带判断**:`SMIX_ANDROID_SERIAL=R5CT52DF07D bash
   scripts/release/android-instrumentation-gate.sh; echo "exit=$?"` → 期望**非零**,
   且**在发出任何 adb / gradle 命令之前**返回。
   **这条是本步最要紧的一条**:脚本一旦被 `bash scripts/…` 包起来,
   adb-guard 就看不见里面的命令文本了(它判的是这一条命令的文本,而这条里没有
   `adb` / `gradlew`)。**把判断藏进脚本等于绕过 guard,除非脚本自带同一份判断。**

**绿(实现 + 接线)**

- 新文件:`scripts/release/android-instrumentation-gate.sh`
  - 头注释按本仓惯例写**它防的是哪一次事故**,而不是它检查什么:`:sdk` 的 4 条断言
    从写下到 C2 之前从未被任何闸门跑过;以及 `:app` 为什么不在这里(C2 的 3 分 40 秒)
  - 设备选择,优先级与 corpus gate 同源:`SMIX_ANDROID_SERIAL` → 否则取
    `adb devices` 里第一个 `emulator-*`;**非 `emulator-[0-9]+` 形态一律拒绝**
    (与 adb-guard 同判据:白名单 emulator 形态,而不是拉黑某台手机 —— 新插上的手机
    默认安全);一台都没有则失败,并打印起 emulator 的那条命令
  - 跑 `ANDROID_SERIAL="$SERIAL" ./gradlew :sdk:connectedDebugAndroidTest --console=plain`,
    **模块限定**在这里是对的:它要跑的就是这一个模块,裸名会把 `:app` 一起拖进来
    而那正是永不返回的那个
  - **截止时间(本步的存在理由之一)**:本机无 `timeout` / `gtimeout`(C2 实测),
    自己实现 —— gradle 放后台,循环 `kill -0` 轮询,超过 `SMIX_ANDROID_GATE_TIMEOUT_S`
    (默认 600)就 `kill` 并以具名消息失败。**一个发布闸门允许失败,不允许挂起**;
    这是 C2 那 3 分 40 秒教的,即便这次跑的是 0.3 秒就结束的套件
  - 跑完调判据脚本,把 `--module` 与 `--results` 都写清
  - 成功打印一行说清跑了什么:`android instrumentation gate: :sdk 4/4 on emulator-5554`
- 新文件:`scripts/dev/androidtest-xml-judge.py`
  - 期望值**从磁盘导出**:数该模块 `src/androidTest/` 下的 `@Test` 个数,不写死 4
  - 结果值:该结果目录下所有 `TEST-*.xml` 的 `<testsuite>` 属性求和
  - 判据四条:结果文件非空 / `tests == 期望` / `failures == 0 且 errors == 0` /
    `skipped == 0`。每条失败消息自带修法,非零退出
- 接线:`scripts/release/ship.sh`,紧接 S1 那段之后加 `--- android instrumentation (device)` 段,
  **非 bypass**,与 swift / cargo 两段同级。放在这里而不是靠近 corpus gate,是为了
  **没有 emulator 时尽早失败** —— 在 fuzz / clippy / semver 那些长活之前,更在任何
  publish 之前。fail 消息给出日志路径与起 emulator 的命令。

**重构**

- 发现但**不在本段修**,如实记账(§8.1):`scripts/release/corpus-gate.sh:116` 用了
  `timeout`,而本机没有这个二进制 —— 那条路径上每个 yaml 都会被判 FAIL。它属于 iOS
  corpus 闸门,不属于 v2.2 的 Android 行为闸门。记进 `docs/v2.md` 决策日志一句,
  并写进本段的收尾报告,由用户定夺。

### S3. `android-gate-scan.py`:按**种类**要求覆盖,`DEFERRED` 就此消失

前两步把线接好了,但闸门扫描此刻仍在用 C1 的模型:"有 `src/androidTest/` 就要求
三处都跑 `connectedDebugAndroidTest`,否则进 `DEFERRED`"。这个模型对 `:app` 永远是错的
—— 它要求的动词对那个模块不存在。本步把模型换成**两种种类、各自的要求**,并且
**种类由磁盘证明**,不由常量声明。

**红(四条注入,逐条)**

```bash
cd /Users/doracawl/workspace/goliajp/smix
cp .github/workflows/ci.yml            /tmp/smix-c3-ci.bak
cp scripts/release/ship.sh             /tmp/smix-c3-ship.bak
cp crates/smix-cli/src/runner_android.rs /tmp/smix-c3-runner-android.rs.bak
cp android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt /tmp/smix-c3-rt.bak
```

1. **编译层从某一处掉出去**:把 ci.yml 的 gradle 行退回只有 `testDebugUnitTest` →
   期望非零,消息说清两个模块的 `assembleDebugAndroidTest` 缺在 **CI**(而 preflight/ship 有)
2. **设备层从 ship 掉出去**:把 ship.sh 里调 `android-instrumentation-gate.sh` 的那行去掉 →
   期望非零,消息说清 `:sdk:connectedDebugAndroidTest` 在**发布路径**上无人跑
3. **`am instrument` 的坐标飘了**:把 `runner_android.rs` 的 `SERVER_ENTRY` 改成
   `dev.smix.runner.RunnerTest#runServerForeverX` → 期望非零,消息说清
   "磁盘上没有这个 `@Test`,`am instrument` 会报 `OK (0 tests)`" ——
   **这是本仓第一次有东西守这个跨语言坐标**
4. **有人往 runner 本体里塞断言**:临时给 `RunnerTest.kt` 加第二个 `@Test` →
   期望非零,消息说清 runner 本体模块不得寄存断言(否则下一个人会顺理成章地
   要求它跑 connected 任务,而那会挂)

四条各自还原(`cp` 备份,不走 `git checkout`),复跑 → `android-gate-scan: clean`。

**绿(改扫描)**

- 文件:`scripts/dev/android-gate-scan.py`
- **删掉 `DEFERRED`**。docstring 里留一句它是什么、为什么在 C3 消失:它是 C1/C2 期间
  "这份 androidTest 还没进闸门,去向记在这里"的临时账;账清了就不该留一张空表,
  留着会让下一个人以为"加进 DEFERRED"是一种合法处置
- **种类由磁盘导出**,新解析:
  - 每个模块的 `src/androidTest/` 下,按 `package` + `class` + `@Test` 后的 `fun` 组出
    每个测试的全名 `pkg.Class#fun`
  - 从 `crates/smix-cli/src/runner_android.rs` 读 `SERVER_ENTRY` 常量
  - 该常量必须在**恰好一个**模块里命中,否则失败(注入 3 抓的就是这条)
  - 命中的模块 = **runner-body**;它必须**只有这一个** `@Test`(注入 4)
  - 其余有 `@Test` 的模块 = **assertions**;有 `src/androidTest/` 却一个 `@Test` 都没有
    = 失败(空 source set 是没写完,不是一种种类)
- **按种类要求覆盖**:
  - 所有带 `src/androidTest/` 的模块:`assembleDebugAndroidTest` 必须被
    preflight / CI / ship **三处都**覆盖
  - `assertions`:`connectedDebugAndroidTest` 必须被 **ship** 覆盖。ship 的文本视为
    `scripts/release/ship.sh` **加上它具名调用的委托脚本**
    (新常量 `SHIP_DELEGATES = {"scripts/release/android-instrumentation-gate.sh": "…"}`,
    值写清为什么委托:设备选择与截止时间不该塞进 ship.sh 主体)。
    ship.sh 必须真的按名调用该委托,否则失败(注入 2)
  - `runner-body`:**不要求任何设备层任务**,并把理由印进输出 —— 不是"暂时不要求"
- **输出行必须说出分层与降级**,例如:
  ```
  android-gate-scan: clean — 2 modules; androidTest compile: preflight+CI+ship;
  instrumentation: ship only — :sdk (4 tests) on a pinned emulator; CI has no emulator;
  :app is the runner body (dev.smix.runner.RunnerTest#runServerForever), never a connected task
  ```
  这一行是**降级的留名处**:CI 只到编译层这件事,必须在每次闸门运行时被说出来,
  而不是躺在某份计划的注释里。

**重构**

- `EXEMPT` 常量当前**没有被 `main()` 引用**,是纯散文。本段不改它的行为(§8.1),
  但在它上方点明这一点 —— 一份看起来像判据、其实没参与判断的常量,
  正是本仓这周反复吃亏的形状(注释是断言,代码才是事实)。
- 不统一三处闸门各自的手写清单。C1 已记过一次,仍在范围外。

## Checkpoint C3 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix

# 0. 钉住的 emulator 在,实体机不参与
adb -s emulator-5554 shell getprop sys.boot_completed

# 1. 编译层:两份 androidTest 都编译,且三处闸门都点到了这条命令
( cd android-runner && ./gradlew testDebugUnitTest assembleDebugAndroidTest --console=plain )
test -s android-runner/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk
test -s android-runner/sdk/build/outputs/apk/androidTest/debug/sdk-debug-androidTest.apk
grep -q 'gradlew testDebugUnitTest assembleDebugAndroidTest' scripts/dev/preflight.sh
grep -q 'gradlew testDebugUnitTest assembleDebugAndroidTest' scripts/release/ship.sh
grep -q 'gradlew testDebugUnitTest assembleDebugAndroidTest' .github/workflows/ci.yml

# 2. 设备层:ship 的 instrumentation 闸门真跑一次,且 ship.sh 真的调它
SMIX_ANDROID_SERIAL=emulator-5554 bash scripts/release/android-instrumentation-gate.sh
grep -q 'android-instrumentation-gate.sh' scripts/release/ship.sh

# 3. 判据脚本自己分得清"跑了几个"
python3 scripts/dev/androidtest-xml-judge.py \
  --module android-runner/sdk \
  --results android-runner/sdk/build/outputs/androidTest-results/connected/debug

# 4. DEFERRED 消失,且扫描输出把分层与降级说出来
python3 scripts/dev/android-gate-scan.py
python3 scripts/dev/android-gate-scan.py | grep -q 'instrumentation: ship only'
python3 scripts/dev/android-gate-scan.py | grep -q 'CI has no emulator'
! grep -q '^DEFERRED' scripts/dev/android-gate-scan.py

# 5. 全套本地闸门
bash scripts/dev/preflight.sh
```

期望:**每条命令 exit 0**。具体输出判据:

- 第 0 条输出 `1`
- 第 1 条输出含 `> Task :app:assembleDebugAndroidTest`、`> Task :sdk:assembleDebugAndroidTest`
  与 `BUILD SUCCESSFUL`;两个 APK 非空
- 第 2 条输出 `android instrumentation gate: :sdk 4/4 on emulator-5554`(数字由磁盘导出,
  与 `@Test` 数一致);**若 `:sdk` 将来加了第 5 条断言,这里必须变成 5/5,写死 4 即为不合格**
- 第 3 条输出 4 与 4 两个数,`failures=0 errors=0 skipped=0`
- 第 4 条第一行以 `android-gate-scan: clean — 2 modules` 起头,并含
  `instrumentation: ship only` 与 `CI has no emulator` 两段
- 第 5 条末行 `preflight: clean`

外加**已在 S1/S2/S3 内完成并记录**的九条红向验证(它们改工作树,不放进复跑命令):

- S1:RunnerTest.kt 注入编译错误 → 接线前 preflight exit 0、接线后非零具名
- S2:XML 判据四条(0 个 / 少一条 / skipped=1 / 无结果文件)各自非零且消息具名
- S2:`:sdk` 断言注入失败 → 闸门脚本非零并点名;传物理机 serial → 在发出任何
  adb / gradle 命令之前拒绝
- S3:CI 退回只跑单测 / ship 去掉委托调用 / `SERVER_ENTRY` 改名 / `:app` 加第二个 `@Test`
  → 扫描各自非零且消息具名

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.2-c3-hot.md`
2. `docs/v2.md` 决策日志追加(§10):`:app` 的闸门形态定为 `assembleDebugAndroidTest`
   (iOS `build-for-testing` 的对等物,编译主体而不起设备)、`:sdk` 的 instrumentation
   进 ship 且判据由磁盘 `@Test` 数导出、**CI 与 preflight 只到编译层是一次明写的降级
   且印在闸门输出里**、`DEFERRED` 表消失、新增跨语言坐标闸门(`SERVER_ENTRY` ↔ 磁盘
   `@Test`)、以及 corpus-gate 依赖不存在的 `timeout` 这条待定记账。
   **追加时注意**:决策正文若写到被 adb-guard 拦的命令形状,heredoc 正文会被 guard
   当命令读(07-21 已发生过一次)—— 用不含那些形状的措辞,或改用编辑工具写入,
   **不改 guard**
3. 回收 emulator:`adb -s emulator-5554 emu kill`,清掉 pid-registry 里的 handle
4. 调 sub-agent 生成新 `plan-hot.md`(到 C4:行为 smoke,真设备上逐条验证 07-20/21 的
   三个修复),按 CLAUDE.md §6 模板 + `docs/plan-cold/v2.2-android-behavioural-gate.md`。
   **交给 C4 的输入**:C4 是本版本唯一涉及产品行为的一段,走真设备实测而非红绿;
   它跑的 yaml 要能在装回缺陷版本时变红,否则它只是又一次"跑通了"
