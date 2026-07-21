# plan-hot — v2.2 到 C2:两份 androidTest 各自真跑一次,并说清它们各是什么

## 目标 checkpoint

C2:`:app` 与 `:sdk` 两份 `src/androidTest/` 都在钉住的 emulator 上真被执行过一次,
且**执行结果的真实数字被记下来**(通过 / 失败 / 跳过 + 失败清单),而不是被处理成"全绿"。

本段的产出是**事实**,不是闸门:C2 结束时 `android-gate-scan.py` 的 `DEFERRED` 里
`:app` 与 `:sdk` 两条**仍在**——接进闸门是 C3 的事。C2 只回答两个问题:它们跑不跑得起来,
以及它们现在说什么。

热化前的本机探测已经把 C2 的形状改掉了一半,先记在这里,后面的步骤按它写:

- **`:app:assembleDebugAndroidTest` 与 `:sdk:assembleDebugAndroidTest` 都是绿的**
  (本机实测,`BUILD SUCCESSFUL`,只有 15 条 `AccessibilityNodeInfo.recycle()` 的
  deprecation 警告)。冷计划担心的"1241 行大概率连编译都过不了"**不成立**,所以 C2
  的第一个 step 不是修编译。
- **`:app` 的 `src/androidTest/` 根本不是断言套件**。`RunnerTest.kt` 1241 行里只有
  **一个** `@Test`——`runServerForever()`,它拉起 NanoHTTPD 然后
  `CountDownLatch(1).await()` 永久阻塞;其余 1200 行是 `SmixHttpServer` 的路由实现,
  也就是 **Android runner 的产品本体**,只是住在 androidTest source set 里(这是
  Maestro/Detox 同款形态,`app/build.gradle.kts` 顶部注释写明了)。
  于是 `connectedDebugAndroidTest` 对 `:app` 是**错的动词**:它不会失败,它会永不返回。
  S2 的红就是把这件事钉成可判断的证据,因为 C3 要不要/怎么把 `:app` 接进 ship gate,
  完全取决于它——一个永不返回的任务接进 `ship.sh` 就是发布脚本永久挂起。
- 真正的断言套件在 `:sdk`:`Spike001Test`(3 个真断言)+ `ConformanceHarnessTest`
  (1 个 emit harness,**设计上恒 pass**),共 **4 个 `@Test`**。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix

# 1) 护栏与源码闸门就位
bash scripts/dev/adb-guard.test.sh          # 期望:all cases pass
python3 scripts/dev/android-gate-scan.py    # 期望:android-gate-scan: clean
python3 scripts/dev/workflow-scan.py        # 期望:workflow-scan: clean

# 2) 没人在用 Android 工具链
pgrep -fl 'emulator|mobilegate'                            # 期望:空
( cd android-runner && ./gradlew --status --console=plain ) # 期望:无 BUSY daemon

# 3) 看清楚现在连着什么
adb devices
```

**第 3 条不是走过场。** 热化时实测:`adb devices` 当前列出
`R5CT52DF07D`(实体机,USB)与 `adb-R5CT52DF07D-S652U2._adb-tls-connect._tcp`
(同一台,无线)。冷计划把"物理手机在场"写成风险,它现在是**在场的事实**——
本段每一条 install / connected / instrumentation 命令的 emulator 钉法都是承重的,
不是仪式。

`ANDROID_SERIAL=emulator-5554` 必须写成**命令行内联前缀**,不能靠先前的 `export`:
adb-guard 判的是这一条命令的文本(`scripts/dev/adb-guard.sh:73-77`),`export` 在
另一条命令里它看不见。被拦时**改命令,不改 guard**。

起 emulator(本段唯一的长驻进程,按 pid-registry 落一个 handle 再进 S1):

```bash
"$ANDROID_HOME/emulator/emulator" -avd sim-smix-android-01 -port 5554 -no-snapshot-save &
adb -s emulator-5554 wait-for-device
adb -s emulator-5554 shell getprop sys.boot_completed   # 期望输出 1
```

AVD 已确认存在(`emulator -list-avds` → `sim-smix-android-01`),
`abi.type=arm64-v8a` / `image.sysdir.1=system-images/android-33/default/arm64-v8a/`。
`-port 5554` 是为了让 serial 确定是 `emulator-5554`,而不是"看它分到哪个"。

## 步骤(线性,无分叉)

### S1. `:sdk` 的断言套件在钉住的 emulator 上真跑一次,如实记账

**红(先证明这套断言真的能红)**

`ConformanceHarnessTest` 的注释白纸黑字写着 "Always pass — test is an emit harness",
所以在把任何"绿"当成信号之前,必须先证明**这个 source set 里存在会红的东西**——
否则 C2 交出的通过率与 `OK (0 tests)` 无法区分。注入对象选 `Spike001Test`
(唯一有真断言的地方)。

```bash
cd /Users/doracawl/workspace/goliajp/smix
BAK=/tmp/smix-c2-spike001.kt.bak
cp android-runner/sdk/src/androidTest/kotlin/dev/smix/sdk/Spike001Test.kt "$BAK"
# 注入:把 testEmptyTreeIdMissByteIdenticalToRust 的期望值改成必然不成立的字面量
```

- 注入后:
  `( cd android-runner && ANDROID_SERIAL=emulator-5554 ./gradlew :sdk:connectedDebugAndroidTest --console=plain ); echo "exit=$?"`
  → 期望 **非零**,且 `androidTest-results` 的 XML 里点名
  `testEmptyTreeIdMissByteIdenticalToRust`
- **还原**:`cp "$BAK" android-runner/sdk/src/androidTest/kotlin/dev/smix/sdk/Spike001Test.kt`
  **禁止用 `git checkout <file>` 还原**——07-21 就是这么把 `viewIdCandidates` 的实现
  一并抹掉的(`docs/v2.md` 决策日志「过程失误」)。备份文件还原,不走 git。

**绿(真跑 + 记账 + 只修真 stale)**

```bash
( cd android-runner && ANDROID_SERIAL=emulator-5554 ./gradlew :sdk:connectedDebugAndroidTest --console=plain )
```

- **第一次跑完,立刻把真实数字写回本文件此处**:`tests= / failures= / errors= / skipped=`
  四个数取自 `android-runner/sdk/build/outputs/androidTest-results/` 下的 `TEST-*.xml`,
  外加逐条失败清单(测试全名 + 失败消息首行)。**先记账,后动手**——"跑完发现是绿的"
  也要把 `4/0/0/0` 写下来,因为 C3 要拿它当基线。
- 每修一条失败,必须在同一处写清**它当初为什么会 stale**,二选一定性:
  - **产品变了** → 测试跟着改,并说明是哪次改动让它失效的(有 commit 就点名)
  - **测试当初就写错了** → 说明错在哪,为什么之前没人发现(答案通常是"没人跑过")
  - 两者都说不清 → **不改**,记成待定并停下报告,不允许用"顺手对齐一下"糊过去
- **不允许为了绿而删测试或加 `@Ignore`**。若某条失败暴露的是真实产品缺陷、且修它超出
  v2.2 的边界(`docs/v2.md`),停下报告用户由其定夺——不自行 defer(§13 + `exec/no-shrink-words`)。
- 已发现的一条 stale **注释**(memory: 注释是断言,代码才是事实),本步顺带修正,理由写在
  改动处:`ConformanceHarnessTest.kt` 头注释点名的两个脚本
  `scripts/sdk/pull-android-conformance.sh` 与
  `scripts/sdk/sync-conformance-fixtures-to-android-assets.sh` **都不存在**
  (`scripts/sdk/` 实有 4 个:`build-android-aar.sh` / `build-xcframework.sh` /
  `regenerate-bindings.sh` / `run-cross-binary-harness.sh`)。同注释说 "24 conformance
  fixtures" 是**对的**:`crates/smix-core-conformance/fixtures/` 与
  `android-runner/sdk/src/androidTest/assets/conformance/` 各 24 个且 `diff -rq` 无差异
  (只多一个 `.gitkeep`)——冷计划正文写的"26 个 fixture"才是错的,一并更正。

**重构**

- 不动 `ConformanceHarnessTest` 的"恒 pass"设计。它按契约就是 emit harness,真正的
  比对发生在 host 侧;把它改成会 fail 的形态是另一件事(且需要先补回那两个不存在的
  pull/sync 脚本),属本段范围外(§8.1),记进 `docs/plan-cold/` 对应版本。

### S2. `:app` 的 androidTest:证明 connected 动词会挂,再用产品自己的起法跑一次

**红(把"错的动词"钉成可判断的证据)**

这一步的被测对象不是断言,是**动词选择**。C3 的默认动作是把
`connectedDebugAndroidTest` 接进 ship gate;必须先证明对 `:app` 这么做会让发布脚本
永久挂起,否则 C3 会照着 `:sdk` 的样子抄一遍然后卡死在第一次发布上。

本机无 `timeout` 也无 `gtimeout`(实测 `which` 皆 not found),所以用后台进程 + 存活
探测来判定,两条证据同时成立才算数:

```bash
cd /Users/doracawl/workspace/goliajp/smix/android-runner
ANDROID_SERIAL=emulator-5554 ./gradlew :app:connectedDebugAndroidTest --console=plain \
  > /tmp/smix-c2-app-connected.log 2>&1 &
echo $! > /tmp/smix-c2-app-connected.pid
```

- **证据 A(它在正常服务,不是卡在安装)**:
  `adb -s emulator-5554 forward tcp:28080 tcp:28080` 后
  `curl -sf -o /dev/null -w '%{http_code}\n' http://localhost:28080/health` → 期望 **200**
  (gradle 的 connected 任务自己**不做** port forward,所以这条 forward 是必需的)
- **证据 B(它永不返回)**:等 180s 后
  `kill -0 "$(cat /tmp/smix-c2-app-connected.pid)"; echo "exit=$?"` → 期望 **exit=0**
  (进程还活着 = 任务没有终止;一个 4 断言的套件早该结束了)。等待用
  `python3 -c 'import time; time.sleep(180)'`,前台 `sleep` 在本环境被禁
- **收尾必须两头都收**,否则 28080 会一直被占着,S2 的绿会误判成 "already healthy":
  ```bash
  kill "$(cat /tmp/smix-c2-app-connected.pid)"
  adb -s emulator-5554 shell am force-stop dev.smix.runner.test
  adb -s emulator-5554 forward --remove tcp:28080
  ```
- 把 A/B 两条的实际输出写回本文件此处。

**绿(用产品自己的起法跑,记录它现在说什么)**

`:app` 这份 source set 的"跑一次"只有一种正确写法,就是产品本身在用的那一条
(`crates/smix-cli/src/runner_android.rs`:`adb -s <serial> install -r -t` → `adb forward`
→ `am instrument -w -e class dev.smix.runner.RunnerTest#runServerForever`)。
不手抄这三段坐标——坐标打错的症状是 `OK (0 tests)`,一个读起来像成功的静默 no-op。

```bash
cd /Users/doracawl/workspace/goliajp/smix
smix runner up emulator-5554 --platform android
curl -sf -o /dev/null -w '%{http_code}\n' http://localhost:28080/health   # 期望 200
curl -sf http://localhost:28080/tree | head -c 400                        # 记录它现在说什么
smix runner down --platform android --device emulator-5554
```

- `/health` 与 `/tree` 的实际响应(状态码 + `/tree` 首 400 字节的形状)写回本文件此处。
  这就是 C2 对 `:app` 的交付物:**它能跑,以及它现在说什么**。
- `/tree` 若返回空树或报错,如实记下并定性,**不在本段修**——那是 C4 的行为 smoke,
  本段不追进去(§8.1)。

**重构**

- `scripts/dev/android-gate-scan.py` 的 `DEFERRED[":app"]` 当前写着
  `"C2 runs it, C3 gates it"`。S2 已证明 `:app:connectedDebugAndroidTest` 不是能被
  gate 的东西,把这条注记改成陈述已证明的约束(动词错在哪、C3 需要另选形态),
  并保留指向 `docs/plan-cold/v2.2-android-behavioural-gate.md` 的出处。
  **条目本身不删**——删是 C3 的动作;留一句已被证伪的话在闸门里,比不写更糟。
  `DEFERRED[":sdk"]` 不动。

## Checkpoint C2 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix

# 0. 钉住的 emulator 在,实体机不参与
adb -s emulator-5554 shell getprop sys.boot_completed

# 1. 两份 androidTest 都编译(不需要设备)
( cd android-runner && ./gradlew :app:assembleDebugAndroidTest :sdk:assembleDebugAndroidTest --console=plain )
test -s android-runner/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk
test -s android-runner/sdk/build/outputs/apk/androidTest/debug/sdk-debug-androidTest.apk

# 2. :sdk 套件在设备上真跑过,且跑的是 4 个断言而不是 0 个
( cd android-runner && ANDROID_SERIAL=emulator-5554 ./gradlew :sdk:connectedDebugAndroidTest --console=plain )
XML="$(find android-runner/sdk/build/outputs/androidTest-results -name 'TEST-*.xml' | head -1)"
test -n "$XML"
grep -q 'tests="4"'    "$XML"
grep -q 'failures="0"' "$XML"
grep -q 'errors="0"'   "$XML"

# 3. :app 的 runner 用产品起法能服务
smix runner up emulator-5554 --platform android
curl -sf -o /dev/null -w '%{http_code}\n' http://localhost:28080/health
smix runner down --platform android --device emulator-5554

# 4. 本段没有把 instrumentation 接进任何闸门 —— 两条 DEFERRED 都还在
python3 scripts/dev/android-gate-scan.py
grep -q '":app"' scripts/dev/android-gate-scan.py
grep -q '":sdk"' scripts/dev/android-gate-scan.py

# 5. 全套本地闸门
bash scripts/dev/preflight.sh
```

期望:**每条命令 exit 0**。具体输出判据:

- 第 0 条输出 `1`
- 第 1 条输出含 `BUILD SUCCESSFUL`
- 第 2 条的 `tests="4"` 是防 `OK (0 tests)` 的非空判据:套件必须真执行了 4 个 `@Test`。
  `failures="0"` / `errors="0"` 只允许由 S1 里**定性并说明了理由**的修复达成,
  **不允许由删测试或 `@Ignore` 达成**
- 第 3 条 `curl` 输出 `200`
- 第 4 条输出 `android-gate-scan: clean — 2 modules; deferred: :app:connectedDebugAndroidTest, :sdk:connectedDebugAndroidTest`
- 第 5 条末行 `preflight: clean`

外加三条**已在 S1/S2 内完成并记录**的验证(它们改工作树或需要 180s 等待,不放进复跑命令):

- 注入一条失败的 `Spike001Test` 断言 → `:sdk:connectedDebugAndroidTest` 非零且 XML 点名该测试
- `:app:connectedDebugAndroidTest` 后台跑 180s 仍存活(`kill -0` exit 0),期间 `/health` 已 200
- `:sdk` 首跑的真实 `tests/failures/errors/skipped` 四个数 + 失败清单 + 每条的 stale 定性,
  已写回本文件 S1

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.2-c2-hot.md`(记账内容随之成为永久记录)
2. `docs/v2.md` 决策日志追加(§10):`:app` 的 androidTest 是 runner 本体而非断言套件、
   `connectedDebugAndroidTest` 对它是错的动词,以及 `:sdk` 首跑的真实数字。
   **追加时注意**:决策正文若写到被 adb-guard 拦的命令形状,heredoc 正文会被 guard
   当命令读(07-21 已发生过一次)——用不含那些形状的措辞,或改用编辑工具写入,**不改 guard**
3. 回收 emulator:`adb -s emulator-5554 emu kill`,并清掉 pid-registry 里的 handle
4. 调 sub-agent 生成新 `plan-hot.md`(到 C3:instrumentation 进 ship gate),按 CLAUDE.md §6
   模板 + `docs/plan-cold/v2.2-android-behavioural-gate.md`;**必须把 S2 的结论作为 C3 的
   输入交进去**——C3 不能对 `:app` 直接套用 `:sdk` 的接法
