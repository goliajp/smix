# plan-hot — v2.4 到 C3:启动 Activity 不再是一个写死的猜测

## 目标 checkpoint

**C3**:`docs/guide-executability.md` 的 **N2 行从 `broken` 转 `runs`**,装回缺陷时闸门重新变红。
Android 上启动一个应用**不再依赖它的启动 Activity 恰好叫 `.MainActivity`**:
默认由系统解析,`apps.yaml` 的 `activity:` 作为显式覆盖真正生效。

## 前置条件

```bash
git status --short
# 期望:空(C2 已提交)

cargo test -p smix-cli --bin smix guide_gate 2>&1 | grep 'test result:'
# 期望:ok. 13 passed

grep -c '| N2 |.*| broken |' docs/guide-executability.md
# 期望:1 —— 缺陷仍在

grep -c 'MainActivity' android-runner/app/src/main/kotlin/dev/smix/runner/RunnerWire.kt
# 期望:≥1 —— 字面量仍钉在那里

pgrep -fl 'runner.ts|smix run|supervise'
# 期望:空。非空 = 有人的 batch 在跑,停下等它结束(memory: runner_ops_check_batch_owner_first)
```

---

## 本段预先定死的四个口径(执行期不得再议)

### 口径 1 — 根因是「runner 不会找启动 Activity」,不是「用户不能配置」

按 §12.2 第一步:这格能力 core 有吗?**没有**。`RunnerWire.foregroundCommand` 把
`am start -n $bundleId/.MainActivity` 写死,注释自陈这是「约定」——
**对启动 Activity 不叫这个名字的应用(含全部 AOSP 应用)一律无效**。

于是修法**不是**「把 `activity:` 从 yaml 传到设备就完事」。那只是把「猜一个名字」换成
「要求用户报一个名字」,而这个名字**系统本来就知道**。正统做法是问系统:
instrumentation 里有 `Context`,`packageManager.getLaunchIntentForPackage(pkg)` 直接给出
启动 Activity 的 `ComponentName`。零配置、对每个应用都对。

`activity:` 覆盖仍然要生效 —— 有多个 LAUNCHER 类别 Activity、或要从某个特定入口进的应用
需要它。**两件事一起做才让指南那句话成真**:默认解析负责普通情形,覆盖负责其余。

### 口径 2 — 覆盖走 header,与 bundle id 同一机制

`App-Bundle-Id` 已经是每请求随 header 走的(runner 按它 `resolveApp()` 重绑)。
Activity 覆盖用 **`App-Launch-Activity`** header,同一条路。

**不**新开 body 字段:`/session/launch-app` 与 `/foreground` 与 `/session/relaunch-app`
三条路由都要用到它,body 形状各不相同,而 header 对三条一视同仁 ——
`RunnerWire.kt:159-162` 的注释正是这么写的(「需要把约定扩展,而不是给每条路由分个叉」)。

### 口径 3 — Kotlin 侧的解析必须有单元测试,且不依赖设备

`RunnerWire` 是纯函数集合,`app/src/test/` 下的 `RunnerWireTransformTest` 已经在测它
(`foregroundCommandTargetsMainActivitySingleTop` 就是钉 `.MainActivity` 的那条)。

因此:
- `foregroundCommand(bundleId, activity: String?)` 保持**纯函数** —— 传入已解析好的 activity,
  `null` 时才落回 `.MainActivity`
- **解析本身**(`getLaunchIntentForPackage`)在 `RunnerTest.kt`(runner body,instrumentation 内)
  做,那里有 `Context`
- 那条钉 `.MainActivity` 的既有单测**改成钉新契约**,不是删掉:显式 activity → 用它;
  `null` → 落回旧字面量。旧行为是 fallback 而不是唯一行为,测试要说出这个差别

### 口径 4 — 需要设备的部分与不需要的分开,先做不需要的

**不需要设备**(先做,能在本机判定):
- Rust:`apps_config` 的 activity 走到 driver、再走上 wire
- Kotlin:`RunnerWire.foregroundCommand` 的签名与 fallback,`app/src/test/` 单测
- 闸门:N2 的 probe 从「配置的 activity 到不了设备」翻成「到得了」

**需要设备**(后做):
- `getLaunchIntentForPackage` 的实际解析结果
- 一条真的对 AOSP 应用(启动 Activity 不叫 `.MainActivity`)的 `launchApp`

设备段起之前**必须**重跑前置条件里的 `pgrep`;Android 侧没有 iOS 那样的显式-UDID 护栏,
`gradlew install*` 会装到**所有**连着的设备,所以先 `export ANDROID_SERIAL=<emulator>`
并在每条 adb 命令里显式带 `-s`(memory: `android_gradle_installs_to_all_devices`)。

---

## 步骤(线性,3 个)

### S1. 让「配置的 activity 到得了设备」先有断言

**红(写测试)**

- 文件:`crates/smix-cli/src/guide_gate.rs`
- 把 `a_configured_launch_activity_still_reaches_nothing` 翻成正向,改名
  `a_configured_launch_activity_reaches_the_device`:跑那条 `activity: .NotMainActivity` 的流,
  断言轨迹里**出现**这个字符串
- 跑:应看到红

**绿(实现)**

- 文件:`crates/smix-adapter-maestro/src/apps_config.rs` —— `resolve_app_into_flow` 目前只写
  `flow.app_id`;让它把 Android 的 activity 也带出来
- 文件:`crates/smix-adapter-maestro/src/runtime.rs` + `crates/smix-sdk`(`LaunchAppOptions`)——
  activity 随 launch 走到 `AppLike`
- 文件:`crates/smix-runner-client/src/lib.rs` —— 多一个可选 header `App-Launch-Activity`
- 文件:`crates/smix-driver/src/android.rs` —— 把它挂上去
- 跑:S1 那条转绿

**重构**

- 无。

### S2. Kotlin 侧:先解析,解析不出来才落回旧约定

**红(写测试)**

- 文件:`android-runner/app/src/test/kotlin/dev/smix/runner/RunnerWireTransformTest.kt`
- 把 `foregroundCommandTargetsMainActivitySingleTop` 改成两条:
  显式 activity → 命令里是它;`null` → 命令里是 `.MainActivity`
- 跑:`./gradlew :app:testDebugUnitTest`,应看到红(签名还没变)

**绿(实现)**

- 文件:`android-runner/app/src/main/kotlin/dev/smix/runner/RunnerWire.kt` ——
  `foregroundCommand(bundleId: String, activity: String?)`
- 文件:`android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt` ——
  五个调用点改为传入解析结果:先读 `App-Launch-Activity` header,没有则
  `context.packageManager.getLaunchIntentForPackage(pkg)?.component?.className`,
  再没有则 `null`(落回旧约定)
- 跑:`./gradlew :app:testDebugUnitTest` 绿;`./gradlew :app:assembleDebugAndroidTest` 编得过

**重构**

- 无。

### S3. 表、文档、验红

**红(写测试)**

- 文件:`docs/guide-executability.md` —— N2 行:`status` → `runs`,`probe` 换新名,`层` → `—`,
  `依据` 换成**修复代码**的引用,`复核` → 当天
- **装回缺陷验红**:把 `RunnerWire.foregroundCommand` 改回忽略参数、恒用 `.MainActivity`,
  Kotlin 单测必须红;把 Rust 侧的 header 摘掉,闸门必须红。两处结果都写进决策日志

**绿(实现)**

- 文件:`docs/ai-guide/08-cookbook.md` —— `apps.yaml` 示例旁写明 `activity:` 是**覆盖**,
  省略时由系统解析
- 文件:`docs/ai-guide/05-cli.md` 若提到 Android 启动约定,同步
- 文件:`docs/v2.md` 决策日志按 §10 追加一行,写明口径 1 的根因判断与两处验红结果
- 跑:`bash scripts/dev/preflight.sh`

**重构**

- 无。

---

## Checkpoint C3 验收

```bash
cargo test -p smix-cli --bin smix guide_gate -- --nocapture 2>&1 | grep -E 'guide-executability:|test result:'
grep -c '| N2 |.*| runs |' docs/guide-executability.md
bash scripts/dev/preflight.sh
```

期望:

1. 摘要行为 `guide-executability: 8 claims (4 runs / 4 broken / 0 unjudged) · … 69 yaml blocks judged`;
   且 `test result: ok. … 0 failed`
2. 第二条输出 `1`
3. 第三条最后一行 `preflight: clean`

设备段的结果**不进 checkpoint 判据**(§5:半年后重跑要能给出确定结论,而「当时那台模拟器上跑通了」
给不出),但必须写进决策日志。

## 完成后动作

1. `mv docs/plan-hot.md docs/plan-history/v2.4-c3-hot.md`
2. 生成新 `docs/plan-hot.md`(覆盖 C4:清单里剩余各条 —— N3 默认 tap 的路由声明、
   N5 `pressKey` 的键名、N6 `assertTrue` 的关系运算符、N7 regex 自动识别),附加专属 context:
   - N5 的形状已经查清:`- back` 才是导航返回的动词(`parser.rs:2499`),`lock` 是
     `SCREEN_LOCK` 的真名,`POWER` 没有对应物;并且 `pressKey: VOLUME_UP` 在 iOS 模拟器上是
     **skip 不是执行**(Apple 的 XCUIDevice.Button 限制),页面那句「Available keys」在这一点上
     也误导
   - N6 与 N7 都是「文法比页面窄」:一个缺关系运算符,一个只认 `|`。两条各自要先定
     「补文法还是收窄页面」,判据同 C2 —— 先问是不是 core 能力缺位
