# plan-hot — v2.2 到 C1:Android 的 JVM 单测有地方失败

## 目标 checkpoint

C1:`:app:testDebugUnitTest` 进 preflight / CI / ship 三处闸门 —— 那 8 个测试文件
(含为修 view-id 缺陷刚写的 `ViewIdCandidatesTest.kt`)从今往后能让某个东西变红。
并且「还有没有别的 gradle 模块测试任务落在闸门之外」由**脚本每次重新枚举**回答,
不靠这份计划里的清单,也不靠谁的记忆 —— 新模块、新 `src/test/`、新 `src/androidTest/`
出现时闸门自己会说话。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix
bash scripts/dev/adb-guard.test.sh        # 期望:all cases pass
python3 scripts/dev/workflow-scan.py      # 期望:workflow-scan: clean
pgrep -fl 'emulator|mobilegate'           # 期望:空
cd android-runner && ./gradlew --status --console=plain   # 期望:无 BUSY daemon
```

本段**不触碰任何设备**。全部工作在 JVM 单测与源码扫描层,`connected*` / `install*` /
`am instrument` 一条都不跑 —— 那是 C2/C3。adb-guard 若拦下什么,说明写错了命令。

## 步骤(线性)

### S1. 把 `:app:testDebugUnitTest` 接进 preflight / CI / ship

**红(证明缺口在,再证明接线后会红)**

被测对象是闸门本身,所以「红」不是写业务测试,是**注入一个真实失败**,看闸门在
接线前后的两种反应。顺序不可颠倒:先证明现在抓不到,否则无从判断接线是否起了作用。

```bash
cd /Users/doracawl/workspace/goliajp/smix
BAK=/tmp/smix-c1-vic.kt.bak
cp android-runner/app/src/test/kotlin/dev/smix/runner/ViewIdCandidatesTest.kt "$BAK"
# 注入:把该文件里任意一条断言的期望值改成必然不成立的字面量
```

- **接线前**:`bash scripts/dev/preflight.sh; echo "exit=$?"` → 期望 **exit=0**
  (缺口证实:一个真实失败的 Android 单测,本地闸门全绿地放过去了)
- **接线前**:`grep -n 'gradlew' .github/workflows/ci.yml scripts/release/ship.sh`
  → 期望只见 `:sdk:testDebugUnitTest` 与 `:sdk:publish`,`:app` 一处都没有
- **接线后**:`bash scripts/dev/preflight.sh; echo "exit=$?"` → 期望**非零**,且输出里
  出现 `ViewIdCandidatesTest` 与具名失败消息
- **接线后**(等价于 ship.sh 与 CI 跑的同一条命令):
  `( cd android-runner && ./gradlew testDebugUnitTest --console=plain ); echo "exit=$?"`
  → 期望**非零**
- **还原**:`cp "$BAK" android-runner/app/src/test/kotlin/dev/smix/runner/ViewIdCandidatesTest.kt`
  **禁止用 `git checkout <file>` 还原** —— 07-21 就是这么把 `viewIdCandidates` 的实现
  一并抹掉的(决策日志「过程失误」),那个文件当时整体未提交。备份文件还原,不走 git。
- **还原后**:`bash scripts/dev/preflight.sh` → 期望 exit 0,末行 `preflight: clean`

**绿(接线)**

三处都用**裸任务名** `testDebugUnitTest` 而不是逐个模块点名。已实测(本机
gradle 9.3.1):裸任务名解析到 `:app:testDebugUnitTest` 与 `:sdk:testDebugUnitTest`
两个 —— 将来新增模块自动进闸门,不需要谁记得回来改这一行。C1 的下半段
(S2)负责在这个前提失效时报警。

- 文件:`scripts/dev/preflight.sh`
  - 在 rust 段之后、`--- source gates` 之前插入 `--- android unit tests` 段:
    `( cd android-runner && ./gradlew testDebugUnitTest --console=plain )`
  - **无条件跑,不按 git diff 收窄**。crate 段按 diff 收窄是因为 clippy 全工作区太慢;
    Android 单测不能照抄这个:v2.2 存在的三个缺陷全是**跨语言契约缺陷**(Rust 侧
    发头 / Android 侧不读),改动只落在 `crates/` 时 Android 断言照样会失效。按
    `android-runner/` 的 diff 收窄等于把这类缺陷继续放过。§13:研发成本是最不重要
    的维度,增量构建热 daemon 下这一步不到 1 秒。
- 文件:`.github/workflows/ci.yml`
  - job `kotlin-sdk` → 更名 `android-unit`(名字仍说 sdk 会重新制造"Android 被覆盖了"
    的错觉,那正是本段要消灭的东西)
  - step 名 `unit tests (sdk + app)`,run 改为 `./gradlew testDebugUnitTest --console=plain`
- 文件:`scripts/release/ship.sh`(第 83-91 行那一段)
  - 命令改为 `./gradlew testDebugUnitTest --console=plain`
  - `log` 与 `fail` 文案去掉 "sdk" 限定,fail 消息保留日志路径
  - 该段注释补一句为什么加 `:app`:8 个 JVM 测试文件此前无人跑,包括为修
    `--force-key-events` 空实现与 view-id 占位符拼法刚写的那几个

**重构**

- ship.sh 该段现有注释讲的是"bindings 首次编译发生在 publish 期"的旧账,仍然成立,
  保留;新增一句不覆盖它。
- preflight.sh 顶部注释说明的是"为什么按 diff 收窄 crate",新加的 android 段不属于
  那个逻辑 —— 在段内写清它为什么无条件,免得后来者按上面的模式把它也收窄掉。

### S2. 闸门扫描:不许再有 gradle 测试任务落在闸门之外

C1 的下半段。清单靠人维护必然过期(v2.2 的存在本身就是证据),所以由脚本从
`settings.gradle.kts` 与磁盘上的 source set **每次重新枚举**,再与三处闸门实际
调用的命令对账。

**红(注入验证,三条 check 逐条)**

写完脚本先注入,再看它抓不抓得到。三条 check 各有独立注入,一条都不能省 ——
只跑一次"当前绿"就收工等于加了一道形同虚设的检查(决策日志 2026-07-20
「闸门必须用真实注入验证,而不是'看起来能抓'」)。

```bash
cd /Users/doracawl/workspace/goliajp/smix
cp android-runner/settings.gradle.kts /tmp/smix-c1-settings.bak
cp scripts/release/ship.sh            /tmp/smix-c1-ship.bak
```

1. **新模块溜过去**:`settings.gradle.kts` 临时加 `include(":probe")`,
   `mkdir -p android-runner/probe/src/test/kotlin` →
   `python3 scripts/dev/android-gate-scan.py; echo "exit=$?"` → 期望非零,
   消息点名 `:probe` 与它缺的那个任务
2. **某模块从闸门里掉出去**:把 ship.sh 的 gradle 行临时改回 `:sdk:testDebugUnitTest` →
   期望非零,消息说清 `:app:testDebugUnitTest` 在 **ship** 闸门外(而 preflight/CI 里有 ——
   三处必须**各自**覆盖,任一处漏掉就是发布路径上的洞)
3. **什么都没扫到却报绿**:把 `include(":app")` 与 `include(":sdk")` 全部临时注释 →
   期望非零,消息说模块数低于下限。这是 v2.1-c7 那道闸门的「抽取下限」判据:
   路径写错→扫到零个→绿,是这类脚本最常见的死法

```bash
cp /tmp/smix-c1-settings.bak android-runner/settings.gradle.kts
cp /tmp/smix-c1-ship.bak     scripts/release/ship.sh
rm -rf android-runner/probe
python3 scripts/dev/android-gate-scan.py   # 期望:android-gate-scan: clean
```

同样**禁止用 `git checkout` 还原**。

**绿(实现 + 接线)**

- 文件:`scripts/dev/android-gate-scan.py`
- 形态对齐 `scripts/dev/workflow-scan.py`:模块 docstring 写清它为什么存在(**它防的是
  哪一次事故**,不是它检查什么)、收集 `failures` 列表、每条失败消息自带修法、
  非零退出、成功打印 `android-gate-scan: clean`
- **不调用 gradle**。CI 的 `source-gates` job 跑在 ubuntu 上,没有 JDK 也没有 Android
  SDK,`./gradlew tasks` 在那里跑不起来。枚举全部来自源码:`settings.gradle.kts` 的
  `include(":x")` 行 + 每个模块目录下 `src/test/` `src/androidTest/` 是否存在
- Checks:
  1. **模块数下限**:枚举到的模块少于 2 个即失败(当前是 `:app` `:sdk`)。防"扫了个空"
  2. **有 `src/test/` 的模块**,`testDebugUnitTest` 必须被 `scripts/dev/preflight.sh`、
     `.github/workflows/ci.yml`、`scripts/release/ship.sh` **三处都**覆盖。覆盖判定:
     解析这三个文件里的 `gradlew <tasks…>` 调用,裸任务名视为覆盖所有模块,
     `:mod:task` 只覆盖该模块
  3. **有 `src/androidTest/` 的模块**,`connectedDebugAndroidTest` 要么被三处覆盖,
     要么在脚本内的 `DEFERRED` 常量里具名,值写清**推到哪个 checkpoint、依据哪份冷计划**。
     没在覆盖里、也没在 `DEFERRED` 里 = 失败。当前 `:app` `:sdk` 两个都进 `DEFERRED`
     (C2 编译 + 真跑,C3 进 ship gate),等 C3 落地时把条目删掉即可
  4. **脚本自身被三处调用**:三个闸门文件里都要出现 `android-gate-scan`,否则报它自己
     inert。adb-guard 就是"脚本进了库、让脚本运行的那一行没进"死的(决策日志 07-21)
- **具名豁免**,写成常量 + 一句理由,不是沉默不查:`testReleaseUnitTest` 与
  `testDebugUnitTest` 跑的是同一份 `src/test/` 源码,debug 变体已覆盖;`test`(聚合)、
  `build` / `buildNeeded` / `buildDependents`(链式带上单测)同理不单独要求;
  `deviceAndroidTest` / `deviceCheck` 走 Device Provider,与
  `connectedDebugAndroidTest` 同一批设备测试,由 check 3 一并管
- 接线三处:
  - `scripts/dev/preflight.sh` 第 59 行 gate 循环加 `android-gate-scan`
  - `.github/workflows/ci.yml` 的 `source-gates` job 加一步
  - `scripts/release/ship.sh` 在 route conformance 那段旁边加一段,fail 消息说清含义

**重构**

- 三处闸门此时各有 5 个 source gate,`preflight.sh` 的循环、CI 的 job、ship.sh 的
  逐段调用是三份手写清单。本段**不**统一它们 —— 那是范围外的改动(§8.1),
  发现了就记进 `docs/plan-cold/` 对应版本,不在这里动。

## Checkpoint C1 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix

# 1. 两个模块的 JVM 单测都真的执行了 —— 结果文件是执行的证据,不是命令的回声
( cd android-runner && ./gradlew testDebugUnitTest --console=plain )
test -s android-runner/app/build/reports/tests/testDebugUnitTest/index.html
test -s android-runner/sdk/build/reports/tests/testDebugUnitTest/index.html
ls android-runner/app/build/test-results/testDebugUnitTest/TEST-*.xml >/dev/null

# 2. 闸门扫描自身干净
python3 scripts/dev/android-gate-scan.py

# 3. 三处闸门都点到了 android 单测与扫描脚本
grep -q 'gradlew testDebugUnitTest' scripts/dev/preflight.sh
grep -q 'gradlew testDebugUnitTest' scripts/release/ship.sh
grep -q 'gradlew testDebugUnitTest' .github/workflows/ci.yml
grep -q 'android-gate-scan' scripts/dev/preflight.sh
grep -q 'android-gate-scan' scripts/release/ship.sh
grep -q 'android-gate-scan' .github/workflows/ci.yml

# 4. 全套本地闸门
bash scripts/dev/preflight.sh
```

期望:**每条命令 exit 0**。具体输出判据:

- `./gradlew testDebugUnitTest` 输出含 `> Task :app:testDebugUnitTest`、
  `> Task :sdk:testDebugUnitTest` 与 `BUILD SUCCESSFUL`
- `android-gate-scan.py` 输出 `android-gate-scan: clean`
- `preflight.sh` 末行 `preflight: clean`

外加两条**已在 S1/S2 内完成并记录**的红向验证(它们改工作树,不放进复跑命令):

- 注入一条失败的 `:app` 单测 → 接线前 preflight exit 0、接线后 preflight 非零
- 注入 `:probe` 模块 / ship.sh 退回 `:sdk:` / 清空 include 三种情况 →
  `android-gate-scan.py` 各自非零且消息具名

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.2-c1-hot.md`
2. `docs/v2.md` 决策日志追加一行(§10):`:app` 单测入闸门 + 闸门外任务由脚本枚举,
   理由一句。**追加时注意**:决策正文若写到被 adb-guard 拦的命令形状,heredoc
   正文会被 guard 当命令读(07-21 已发生过一次)—— 用不含那些形状的措辞,或改用
   编辑工具写入,**不改 guard**
3. 调 sub-agent 生成新 `plan-hot.md`(到 C2:instrumentation 套件真跑一次),
   按 CLAUDE.md §6 模板 + `docs/plan-cold/v2.2-android-behavioural-gate.md` 的
   本段专属 context;C2 起会碰设备,`ANDROID_SERIAL=emulator-NNNN` 是硬前置
