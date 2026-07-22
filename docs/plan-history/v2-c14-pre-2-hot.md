# plan-hot — v2 到 C14-pre-2:发布前不留一个未亲验的 gate

## 目标 checkpoint

**C14-pre-2**:`ship.sh` 上 crates-publish 之前的**每一道 gate**,都已在一台能跑它的机器上
亲眼跑绿一次 —— 没有一道是「应该会过」。跑完把完整状态交给用户,由用户决定发不发。

上一段(C14-pre)把 ship 跑到了发布边界,但有几道 gate 当时**没能亲验**:
mini 缺 `bun install` 让 TS 段没跑到底、Android gradle 段没在本机跑过、
`cargo-semver-checks` / `ffi-bindings-fresh` / `fact-scan` / `llms` 也没单独确认。
「跳过」和「验过」是两件事,发布前不允许把前者读成后者。

## 前置条件

```bash
git status --short                      # 期望:空
cargo test -p smix-cli --test list_sessions_no_nested_runtime   # 期望 exit 0(上段的修复)
```

## 已经查清、不必重查的事实

- **Rust 全套 + Swift + workflow-scan + fence-check 已绿**(C14-pre / S1,mini 1048 tests)
- **smoke gate 修好后过**(list-sessions 不再 panic)
- **TS vitest 本机已绿**(FailureCode 魔数已改)
- **本机有 `cargo-semver-checks` 与 `android-runner/gradlew`**,两段都能在本机跑

## 本段预先定死的口径

### 口径 — 亲验,不推断;跳过必须写明为什么跳不了

每一道 gate 只有两种合法结局:
1. **在能跑它的机器上跑绿**,记下命令与结果
2. **确实跑不了**(需要 autorun 不该碰的资源,如 Android 物理设备 / 启动 emulator),
   **写明是哪一道、为什么、以及它在别处(CI)由什么覆盖**

**不允许第三种**:「大概会过」。这正是 `build-runner-tarball.sh` 那条注释的病。

### Android 设备 gate 仍然跳过

`android-instrumentation-gate.sh` / `android-behaviour-gate.sh` 需要 emulator,
autorun 不启动它(会污染用户物理机 `R5CT52DF07D`)。这两道**留给 CI**,
本段不跑,但要在验收里点名它们是唯一未亲验的两道及其 CI 覆盖。

## 步骤(线性,2 个)

### S1. 把本机能跑的剩余 gate 逐道跑绿

**红**:无新测试。这一步执行既有 gate。

**绿**:在本机逐道跑,记录命令与退出码 —
- `cargo-semver-checks`(ship.sh:252 段的等价命令)
- `scripts/dev/ffi-bindings-fresh.sh`
- `scripts/dev/fact-scan.py`
- `python3 scripts/dev/gen-llms.py --check`
- `android-runner/gradlew testDebugUnitTest assembleDebugAndroidTest`(JVM 单测 + androidTest **编译**,不装设备)

**关键点**:任何一道非零即停、如实记根因,不绕过。

### S2. 汇总发布前状态,交用户拍板

**红**:无。

**绿**:在 `docs/v2.md` 决策日志记一条,列出 crates-publish 前**每一道 gate** 的亲验结果
(绿 / 跳过+原因+CI 覆盖),并明确:crates.io / npm / Maven / git tag **全部未执行**,
发不发由用户决定。

## Checkpoint C14-pre-2 验收

```bash
git status --short                      # 期望:空
cargo test -p smix-cli --test list_sessions_no_nested_runtime   # 期望 exit 0
```

外加:`docs/v2.md` 新增一条,列出 crates-publish 前每一道 gate 的亲验结果,
未亲验的仅剩两道 Android 设备 gate,且写明其 CI 覆盖。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2-c14-pre-2-hot.md`
2. **发布本身待用户拍板** —— autorun 到此为止能做的都做完了。
   `cargo publish` 等不可撤销且对外,不在 autorun 范围。


---

## 归档记录(2026-07-23,C14-pre-2 通过)

**crates-publish 前每一道能在本机/mini 跑的 gate 都亲验绿了,途中又揪出两个「跳过≠验过」的真问题。**

- **S1(逐道亲跑)**:ffi-bindings ✅ / fact-scan ✅ / **gen-llms 🔴→✅**(`captureDuring` 投影漏了,重生)/ **Android gradle 🔴→✅**(Kotlin `FailureCode` 魔数,与 TS 同族,已修)/ cargo-semver-checks ✅(21 检查通过,4 无基线排除)。
- **S2(交状态)**:crates-publish 前每道 gate 的亲验账已入 `docs/v2.md` 决策日志(2026-07-23),唯二未亲验的是两道 Android 设备 gate(需 emulator,autorun 不启动,CI 覆盖)。
- **验收**:git 干净 + `list_sessions_no_nested_runtime` 绿。

发布本身待用户拍板。细节见 `docs/v2.md` 决策日志 2026-07-23(gen-llms / Kotlin 魔数 / semver 实质通过 / 逐道亲验账)。
