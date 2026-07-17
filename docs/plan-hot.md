# plan-hot — v2 到 C5：Android parity 门禁

## 目标 checkpoint

C5：架构 §05 门禁表 14 行全部落定 —— 每行要么达成 parity，要么**记录**为 re-tier 并写明平台真缺什么原语。通过后世界：「iOS + Android 全 parity 是发布门槛」这句话有逐行证据，而不是 dossier 里的一个承诺。

## 前置条件

```bash
git log --oneline -1                                  # 期望 C4 已提交（6cf0a4315 或其后）
python3 scripts/dev/hygiene-scan.py --noise-only      # 期望 clean
bash scripts/dev/fence-check.sh                       # 期望 clean
pgrep -fl "runner.ts|smix run|supervise"              # 期望空
pgrep -fl "gradle|mobilegate|emulator"                # **C5 必查** —— 要跑 gradle + emulator，kevy 曾长期占用
adb devices                                           # 期望至少一台 emulator
```

## 已确证的起点（C4 收尾时查实）

- **12 个 gate verb 里 8 个在 Android runner 侧完全缺失**：`setPermissions` / `setLocation` / `addMedia` / `startRecording` / `toggleAirplaneMode` / `pasteText` / `copyTextFrom` — 加 `stopRecording`。已存在：`setOrientation` / `doubleTap` / `clearKeychain`。
- **宿主侧杠杆已经在了**：`smix-adb` 有 `pm_grant` / `pm_revoke` / `shell` / `screenshot` / `start_activity` / `force_stop`。`setPermissions` 差的是**接线**，不是能力。
- `AndroidDriver` 只有 2 个方法走 `defer_err`（C1 已把其中的用户可见字符串清干净）。

## 步骤（线性）

### S1. 逐行定性：parity 还是 re-tier

**先做，因为它决定 S2 的工作量。** 对 8 个缺失 verb 各查一次「Android 有没有这个原语」，把答案落进 `docs/v2.md` 决策日志。**这一步只读不写代码。**

| verb | 待查 |
|---|---|
| `setPermissions` | `adb pm grant/revoke` 已在 smix-adb → 接线即可。**parity** |
| `setLocation` / `travel` | emulator console `geo fix` — 需 telnet + auth token，还是 `adb emu geo fix`？ |
| `addMedia` | `adb push` + MediaStore scan broadcast |
| `startRecording` / `stopRecording` | `adb shell screenrecord` 的生命周期（它有 3 分钟上限 —— **这是真约束，可能迫使 re-tier 或分段**） |
| `toggleAirplaneMode` | `adb shell cmd connectivity airplane-mode` (API 30+) vs 老的 settings put + broadcast |
| `pasteText` / `copyTextFrom` | `ClipboardManager` 需在 app 进程内 —— **runner 是独立进程，可能够不到**。这条最可能 re-tier |
| `clearKeychain` | 已定 re-tier（Android 无 Keychain），补文档理由 |

**产出**：每行一句「parity，用 X」或「re-tier，因为 Android 没有 Y」。**不许留空**。

### S2. 实现判定为 parity 的行

**红**
- 文件：`android-runner/.../RunnerTest.kt`（route 层）+ `crates/smix-driver/tests/`（driver 层 mock）
- 断言：每个新 verb 的 route 收到请求 → 调对应 adb/UiAutomator 原语；错误路径 → 明确失败而非静默成功。

**绿**
- 按 S1 的判定实现。**优先 `setPermissions`** —— 杠杆已在，是最短路径，也验证接线模式。
- 关键点：宿主侧的（adb）落 `smix-adb`；设备侧的（UiAutomator）落 android-runner。**别把宿主能力塞进 runner**（§12.1）。

**重构**
- 无。

### S3. re-tier 的行：记录，而非静默

**绿**
- 每个 re-tier verb 在 Android 侧返回**明确错误**，说清「Android 没有这个原语，因为 X；改用 Y」—— 不是静默 no-op。
- `docs/ai-guide/` 的平台差异表补齐（若无则建）。
- **C1 教训**：这些错误字符串是用户可见的，别写内部词汇。

## Checkpoint C5 验收

```bash
cargo test --workspace 2>&1 | grep -c "^test result: ok"        # 期望 ≥128（不回退）
cargo build --workspace 2>&1 | grep -c warning                  # 期望 0
python3 scripts/dev/hygiene-scan.py --noise-only                 # 期望 clean
cd android-runner && ./gradlew test                              # 期望 pass（**首次纳入本 cycle 的 gate**）
# 门禁表逐行有定论：
grep -c "C5.*parity\|C5.*re-tier" docs/v2.md                    # 期望 ≥12
```
期望：全部通过 + 门禁表 14 行各有一句记录在案的判定。

**真机验证**：`./gradlew test` 是单元层。真 emulator e2e 要等 kevy 的 bench 不占用时做，或明记「做了+未验证」。**C3/C4 的教训：mock 与 schema 都证明不了真设备上的事。**

## 完成后动作

1. 归档本文件到 `docs/plan-history/v2-c5-hot.md`
2. 生成新 `plan-hot.md`（到 C6：六破坏变更 + codemod + wire v2），见 CLAUDE.md §6
