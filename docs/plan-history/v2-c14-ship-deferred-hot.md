# plan-hot — v2 到 C14:发布(唯一前置是用户授权)

## 目标 checkpoint

**C14**:v2.0.0 发布到 crates.io + npm + Maven Central + Swift Package tag。
这是 v2 的最后一个 checkpoint,发布完 v2 cycle 闭合。

## 为什么这一段停在这里,不 autorun

发布的每一步都**不可撤销且对外**:

- `cargo publish` —— crates.io 不可 unpublish(只能 yank,版本号永久占用)
- `bun publish` —— npm 发布后 72h 才可撤,且撤了版本号也占用
- `gradle publish` —— Maven Central 不可删
- `git tag swift-v2.0.0 && git push` —— 对外 tag

这类动作按项目规则(memory `feedback_no_pr_git_flow_only`:push/对外属 risky,需用户确认)
与 harness 规则(不可逆 / 对外动作需明确授权)**不在 autorun 范围**。
autorun 能做的都做完了:发布前每一道能亲验的 gate 都绿了。

**这一段不是「等我判断」,是「等用户说发」。**

## 前置条件(autorun 侧已全部满足,列出供发布时复核)

```bash
git status --short                    # 期望:空
grep '^version' Cargo.toml | head -1  # 期望:version = "2.0.0"
```

- Rust 全套(mini,1048 tests)✅
- swift 单测 + UITest build ✅
- TS typecheck + vitest ✅ / Android JVM 单测 + androidTest 编译 ✅
- 所有 `*-scan` + fence-check + route-conformance ✅
- corpus(real sim)✅ / smoke(修 list-sessions 后)✅
- ffi-bindings / llms / clippy / cargo-semver-checks(21 检查通过)✅
- **唯二未亲验**:两道 Android 设备 gate(需 emulator,CI 覆盖)

## 发布时执行(用户授权后,由用户或在用户在场时运行)

**发布机是 studio(本机)** —— 依赖齐全(bun / node / cargo-semver-checks / gradle / GPG key)。
mini 缺 `bun install` 等,不适合当发布机。

```bash
# 一条命令,ship.sh 自己按 DAG 顺序发四个生态,失败即停不留半发布状态
bash scripts/release/ship.sh 2.0.0
```

ship.sh 会**重跑**上面所有 gate(不信任「刚才绿过」),全绿后才依次:
crates.io(拓扑序 26 crates)→ npm → Maven Central → swift tag + push。

**Android 设备 gate 在发布机上需要 emulator**:
`"$ANDROID_HOME/emulator/emulator" -avd sim-smix-android-01 -port 5554 -no-snapshot-save &`
(ship.sh 的 `android-instrumentation-gate.sh` 会用它;这一步 autorun 没跑,发布时必须过)。

## Checkpoint C14 验收(发布后)

```bash
cargo search smix-cli | head -1          # 期望:2.0.0
npm view @goliapkg/smix version          # 期望:2.0.0
git tag -l 'swift-v2.0.0'                # 期望:有
```

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2-c14-hot.md`
2. v2 cycle 闭合 → 起 v2.1 或 post-v2 roadmap(见 `docs/roadmap.md`)
