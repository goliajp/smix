# plan-hot — v2.8 到 C6：Android 运行时 parity（rate-limit pacer + app-alive cache）

## 目标 checkpoint

C6：把两个 iOS-specific 的 v1.0.4 运行时特性移植到 Android 的 UiAutomator 路径 ——
① 截图 **rate-limit pacer**（interval floor + slow-path lift，防抓帧过载）；
② **app-alive cache**（记录被测 app 是否还活着,一个探针失败不误判整体死亡）。
Android 的失败模式与 iOS 不同,但 sense/act 层契约可移植。能力 checkpoint。

## 前置条件

```bash
git status --short                                 # 期望：空
grep '^version' Cargo.toml | head -1               # 期望：2.0.0
python3 scripts/release/stress-select.py --test    # C5 仍绿
ls android-runner/app/src/main/kotlin/dev/smix/runner/   # Android runner 在
```

## 已经查清、不必重查的事实

- **Android runner**：`android-runner/app/src/main/kotlin/dev/smix/runner/`（Kotlin,UiAutomator）。
  单测在 `app/src/test/`,instrumentation 在 `app/src/androidTest/`。
- **iOS 参考实现**：`crates/smix-simctl/src/screenshot_pacer.rs`（pacer:interval floor / slow-path lift /
  circuit breaker）+ app-alive cache 在 `swift-bridge/Sources/SmixRunnerCore/SmixRunnerServer.swift`
  （counters + 一个探针失败不 latch 整体死）。
- **Android 现状**：`RunnerWire.kt` 等已有 wire;pacer/alive-cache 是否已有先 grep 确认(S1 起手)。
- **设备**：Android 验证需 emulator。**autorun 不启动 emulator**(会装到用户物理机 `R5CT52DF07D`,
  memory `android_gradle_installs_to_all_devices`)—— 设备 e2e 让位到用户在场 / 显式 `ANDROID_SERIAL`。

## 本段预先定死的口径

- **移植不是照抄 iOS**：Android 的抓帧过载 / app 死亡的失败模式与 iOS 不同,先读 iOS 参考实现的
  **意图**（pacer 防什么、alive-cache 防什么误判）,再按 UiAutomator 的真实失败模式实现。
- **纯逻辑先行**：pacer 的 interval 判定 / alive-cache 的 counter 聚合是纯逻辑,Kotlin 单测钉死
  （`app/src/test/`,不需 emulator）。设备行为进 androidTest（编译进 gate,运行让位）。
- **additive + parity gate**:不破坏现有 Android wire;`route-conformance` / android-gate-scan 守。
  移植后 Android 的这两格能力与 iOS 对齐（parity 表更新一行）。
- **§9#8 三层架构**：pacer/alive-cache 是感知层能力,落 core（Android runner 平铺）,不埋 driver。

## 步骤（线性,2 个）

### S1. rate-limit pacer（UiAutomator 截图）

**红**：Kotlin 单测（`app/src/test/`）—— 给定连续截图请求 + 时间戳,pacer 在 interval floor 内的第二次
请求返回「等待 X ms」而非立即抓;slow-path（上次抓帧慢）时抬高 floor。先跑红。

**绿**：Android pacer（对照 `screenshot_pacer.rs` 的意图,按 UiAutomator `takeScreenshot` 的真实成本调参）
接进 Android 截图路由。additive。

### S2. app-alive cache（探针失败不误判整体死）

**红**：Kotlin 单测 —— 给定「一个探针失败 + app 进程仍在」→ alive-cache 判 app 存活(不 latch 死亡);
「app 进程消失」→ 判死。counter 聚合纯逻辑,先跑红。

**绿**：Android app-alive cache（对照 iOS `AppAliveCache` counters 的意图,按 Android app 死亡的真实
信号:`am`/pgrep app 进程 / UiAutomator window 消失）。接进 runner。androidTest 编译进 gate。

## Checkpoint C6 验收

```bash
( cd android-runner && ./gradlew :app:testDebugUnitTest ) 2>&1 | tail -3   # pacer + alive-cache 单测绿
( cd android-runner && ./gradlew assembleDebugAndroidTest )                # androidTest 编译进 gate
python3 scripts/dev/route-conformance.py                                    # wire additive,parity 不破
# 设备 e2e(emulator,用户在场 / 显式 ANDROID_SERIAL=emulator-5554)：pacer 生效 + alive-cache 不误判
```

期望：单测绿 + androidTest 编译 + route-conformance 绿;设备 e2e 记决策日志。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.8-c6-hot.md`。
2. 决策日志记 Android pacer/alive-cache 的 UiAutomator-specific 实现差异 + parity 表更新。
3. 验收通过后热化 C7（遮挡感知命中判定,EXT1 #4 defer,先调研 z 序可得性）。
