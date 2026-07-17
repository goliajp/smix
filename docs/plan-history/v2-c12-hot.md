# plan-hot — v2 到 C12：两个内部 Rust 改名（`smix-recorder-ir` → `smix-authoring-ir` + `SimctlError` → `DeviceControlError`）

## 目标 checkpoint

C12：破坏性变更 #5（stone crate `smix-recorder-ir` → `smix-authoring-ir`）与 `SimctlError` → `DeviceControlError` 两个**纯内部 Rust 改名**落地 —— 旧名在代码里清零、新名到位、workspace build + test 不回退、两个改名各是一次 crates.io semver break（由 C14 的 `cargo-semver-checks` 兜底）。

通过后世界：`smix-recorder-ir` / `smix_recorder_ir` 在 `crates/` + `scripts/` + `Cargo.lock` 里一处不剩，`crates/smix-authoring-ir/` 就位并可编译发布；`SimctlError` 在全部 Rust 源码里清零、`DeviceControlError` 承接 168 处引用，两平台（simctl / adb）的 device-control 错误类型名不再自称 iOS 工具。

> **本段只做四个改名 break 里风险性质相同的那两个（两次机械改名）。** #1（sessions 强制）、#3（`SMIX_*` 折进 config）、#4（`Modifier(s)` 合并 + `openLink`/`openUrl` 模型）各自带**行为/公开-SDK-API/config 设计决策**，风险性质与"改一个能用的内部符号"不同 —— 与 C6/C7/C8/C9 的拆分同一判据。**C12 是否按此拆分是用户的决定（§10）**，提案见文末「与冷计划不符之处」。

## 前置条件

```bash
git branch --show-current                                    # feature/v2.0
git status --short | grep -c .                               # 0（干净树）
pgrep -fl "runner.ts|smix run|supervise|bun test:e2e" || echo "batch idle"   # 期望空
test -d crates/smix-recorder-ir && echo "recorder-ir 待改名" || echo BAD
test -d crates/smix-authoring-ir && echo "BAD 新名已存在" || echo "authoring-ir 未占用"
git grep -c "SimctlError" -- 'crates/**/*.rs' | awk -F: '{s+=$2} END{print s" SimctlError code refs（进入前基线 168）"}'
cargo test --workspace >/tmp/c12base.out 2>&1; echo "cargo rc=$?"; grep -c "^test result: ok" /tmp/c12base.out
```

## 已确证的起点（本次热化实测，非转述）

**破坏性变更权威表**：`docs/v2.md:32-38` 六项表。#5 = `smix-recorder-ir` → `smix-authoring-ir`（stone crate rename = semver break，迁移 "codemod 改 import"）。`SimctlError` 改名不在六项表里，由冷计划 `plan-cold/v2.md:44` 与 C5 决策日志（`v2.md` 2026-07-17「Android 的错误自称 iOS 工具」）附在 C12。

**① `SimctlError` 的两半：C5 只修了 display 文案，类型名这一半留到了这里。** `v2.md`（2026-07-17 C5）已把 `#[error(...)]` 里硬编码的 `xcrun simctl` 改成 argv 转述，但**类型名 `SimctlError` 本身仍误导**（明确记「改名波及全部调用方，归 …破坏性变更」）。实测：
- 定义 `pub enum SimctlError`（`crates/smix-simctl/src/lib.rs:43`），是**已发布 crate** `smix-simctl` 的公开类型（ship.sh:112 在发布 DAG）。
- **代码引用 = 168 处 / 10 个 Rust 文件**（`git grep -c SimctlError -- 'crates/**/*.rs'` 逐文件求和；与冷计划 `plan-cold/v2.md:44` 的「168 处 / 10 文件」逐字吻合）：`smix-simctl/src/lib.rs` 63 · `smix-sdk/src/android_device.rs` **36** · `smix-sdk/src/ios_device.rs` 24 · `smix-sdk/src/device_control.rs` 23 · `smix-sdk/src/lib.rs` 10 · `smix-simctl/tests/types.rs` 5 · `smix-cli/src/main.rs` 4 · `smix-cli/src/capsule.rs` 1 · `smix-simctl/src/screenshot_pacer.rs` 1 · `smix-simctl/fuzz/fuzz_targets/parse_simctl_output.rs` 1。
- **它服务两个平台**：`android_device.rs` 用它 36 处（`AndroidDeviceControl` 的错误类型），是名字误导最重的证据。
- **新名 `DeviceControlError`：`git grep -c` 全仓 0 命中，无冲突**；且与 `smix-sdk/src/device_control.rs` 的 `DeviceControl` trait 同名族，是最正统的承接名。**由本段 §10 拍板选它**（备选 `DeviceError`，更泛）。
- **不进 FFI**：`smix-ffi/src/lib.rs` 只 export `resolve_selector*`，不碰 `SimctlError` —— 改名**不触发** bindings 重生成、**不触碰任何 Swift/Kotlin/TS 源**。

**② `smix-recorder-ir` 是已发布 stone crate，改名是目录 + 包名 + 依赖方 + 发布清单的机械改。** 实测：
- 目录 `crates/smix-recorder-ir/`，包名 `name = "smix-recorder-ir"`（`Cargo.toml:2`）。**已发布**（`scripts/release/ship.sh:111` 的 `CRATES=(…)` DAG 内）。`smix-authoring-ir` 在代码里**尚不存在**（只在 docs 里作为计划名）。
- **workspace 成员用 glob**（根 `Cargo.toml` 无 `smix-recorder-ir` 显式成员行）—— 目录一改名即自动纳入，无需改成员列表。
- **真依赖方（需改）**：`smix-recorder`（`Cargo.toml:22` `path = "../smix-recorder-ir"` + `use smix_recorder_ir` 见 `src/{lib,session,generator_rust,generator_maestro_yaml}.rs`）· `smix-recorder-ir/fuzz`（自身 `Cargo.toml` 包名 `smix-recorder-ir-fuzz` + dep）· `smix-recorder/fuzz`（`Cargo.toml:20` path dep）· crate 自身 `src/lib.rs`/`tests/ir.rs`/`tests/perf_gate.rs`/`benches/{ir,perf_gate}.rs`。
- **`smix-input/src/lib.rs:6` 只是文档注释里的一句提及**（`//! …shared by smix-driver / smix-recorder-ir / …`），**不是 Cargo 依赖** —— 改文本即可，非依赖图节点。
- 发布清单 `ship.sh:111` 与 `Cargo.lock`（2 处）随改。**crate 自身的 README/CHANGELOG/BUDGETS** 是随 crates.io 发布的文档，一并改；**`docs/` 与 `plan-history/` 的历史引用不改**（那是历史记录，roadmap 指针同步归 C13 docs 段）。
- **不进 FFI**：同 `SimctlError`，与 `smix-ffi` 无关，不触碰非 Rust 源。

**③ 两个改名的用户可见面 = 仅 crates.io semver，无 YAML codemod。** `SimctlError` / `smix-recorder-ir` 都是**内部 Rust 符号 / crate**，不出现在用户 flow 的 YAML 里 —— `smix migrate` 的 YAML codemod **对它们不适用**。破坏性变更表 #5 写的「codemod 改 import」只对**外部 Rust crate 消费者**有意义，而 `smix-recorder-ir` 的消费者全在本仓内部（`smix-recorder` + `smix-input` 文档提及）。真正的用户防线是 **C14 `cargo-semver-checks`**（本机未装，是 C14 门禁；`plan-cold/v2.md:46`）会把这两个 rename 报成 major break。故本段**不扩 codemod**。

**④ 本段零跨语言 blast。** 两个符号都是 Rust-only，不在 FFI 边界、不在三个 SDK 的类型面 —— 验收只跑 Rust（+ clippy/hygiene/bindings-fresh 作不回退证据）。这正是把这两个改名选作 C12 首片的原因：同风险性质、机械、可完全机器判、不牵动 Swift/Kotlin/TS。

## 步骤（线性，无分叉；2 个）

> **TDD 说明**：两个 step 都是**纯改名**，不新增任何行为 —— 按 `plan-cold/v2.md:29`「hygiene/机械类走『build 保持全绿』不变式，不是红绿」。每个 step 的「红」= 本段验收里对应的 grep-gate 现在非 0（证明未改名，已在前置条件实测：`SimctlError` 168、`smix-recorder-ir` 目录在）；「绿」= 改名后该 grep 归 0 且既有测试套保持绿。**不为改名发明假的行为测试**（那会是 §8.5 之外的噪声）。

### S1. `smix-recorder-ir` → `smix-authoring-ir`（stone crate rename，semver break #5）

**红（现状即红）**
- `test -d crates/smix-authoring-ir` 现为假；`git grep -c "smix-recorder-ir\|smix_recorder_ir" -- 'crates/**' 'scripts/**' 'Cargo.lock'` 现非 0。

**绿（实现）**
- `git mv crates/smix-recorder-ir crates/smix-authoring-ir`（保留 git 历史）。
- 改包名 `crates/smix-authoring-ir/Cargo.toml`：`name = "smix-recorder-ir"` → `smix-authoring-ir`。
- 改依赖方 path + crate 名引用：
  - `crates/smix-recorder/Cargo.toml:22`：`smix-recorder-ir = { path = "../smix-recorder-ir", … }` → `smix-authoring-ir = { path = "../smix-authoring-ir", … }`。
  - `crates/smix-recorder/src/{lib,session,generator_rust,generator_maestro_yaml}.rs`：`use smix_recorder_ir::…` → `use smix_authoring_ir::…`。
  - `crates/smix-authoring-ir/fuzz/Cargo.toml`：包名 `smix-recorder-ir-fuzz` → `smix-authoring-ir-fuzz`、`[dependencies.smix-recorder-ir]` → `…authoring-ir`、path。
  - `crates/smix-recorder/fuzz/Cargo.toml:19-20`：dep 名 + `path = "../../smix-recorder-ir"` → `…smix-authoring-ir`。
  - crate 自身 `src/lib.rs` / `tests/{ir,perf_gate}.rs` / `benches/{ir,perf_gate}.rs` / `fuzz/fuzz_targets/ir_action_parse.rs` 里的 `smix_recorder_ir` crate 路径引用。
- 改随发布的 crate 文档：`crates/smix-authoring-ir/{README.md,CHANGELOG.md,BUDGETS.md}`（现含旧名 6/1/3 处），以及 `crates/smix-core/{README,CHANGELOG,BUDGETS}.md` 与 `crates/smix-input/src/lib.rs:6` 文档注释里的旧名提及。
- 改发布清单 `scripts/release/ship.sh:111`：`smix-recorder-ir` → `smix-authoring-ir`。
- `cargo build --workspace` 触发 `Cargo.lock` 重生（旧名两处消失）。

**重构** — 无（改名本身是全部）。

### S2. `SimctlError` → `DeviceControlError`（type rename，semver break）

**红（现状即红）**
- `git grep -c "SimctlError" -- 'crates/**/*.rs'` 现 168（前置条件已实测）。

**绿（实现）**
- 改定义 `crates/smix-simctl/src/lib.rs:43`：`pub enum SimctlError` → `pub enum DeviceControlError`，同文件 63 处引用一并改。
- 改全部调用方（`git grep` 驱动，非手抄）：`smix-sdk/src/{android_device,ios_device,device_control,lib}.rs`（36+24+23+10）· `smix-cli/src/{main,capsule}.rs`（4+1）· `smix-simctl/src/screenshot_pacer.rs`（1）· `smix-simctl/tests/types.rs`（5）· `smix-simctl/fuzz/fuzz_targets/parse_simctl_output.rs`（1）。
- 改随发布的 crate 文档（现含旧名，作为当前 API 文档）：`crates/smix-simctl/{README.md,CHANGELOG.md}` · `crates/smix-sdk/CHANGELOG.md`。**不改** `docs/` / `.claude/rfcs/` / 根 `CHANGELOG.md` 的历史条目（历史记录）。
- 若 `smix-simctl` 有 `pub use`/re-export 旧名，一并改（`git grep "SimctlError" -- 'crates/**/*.rs'` 归 0 即证无残留）。

**重构** — 无。

## Checkpoint C12 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix
# 1. #5 旧名清零（代码 + 脚本 + lock；docs/ 历史保留，不计）
git grep -c "smix-recorder-ir\|smix_recorder_ir" -- 'crates/**' 'scripts/**' 'Cargo.lock' | awk -F: '{s+=$2} END{print "recorder-ir 残留="s+0}'
# 2. #5 新 crate 就位 + 包名 + 发布清单
test -d crates/smix-authoring-ir && echo "dir OK" || echo "dir BAD"
grep -c '^name = "smix-authoring-ir"' crates/smix-authoring-ir/Cargo.toml
grep -c "smix-authoring-ir" scripts/release/ship.sh
# 3. SimctlError 旧名清零 / 新名到位
git grep -c "SimctlError" -- 'crates/**/*.rs' | awk -F: '{s+=$2} END{print "SimctlError 残留="s+0}'
git grep -c "DeviceControlError" -- 'crates/**/*.rs' | awk -F: '{s+=$2} END{print "DeviceControlError="s+0}'
# 4. build + test 不回退（落 /tmp，不接管道读 rc）
cargo build --workspace >/tmp/c12build.out 2>&1; echo "build rc=$?"
cargo test --workspace >/tmp/c12test.out 2>&1; echo "cargo rc=$?"
grep -c "^test result: ok" /tmp/c12test.out; grep -c "^test result: FAILED" /tmp/c12test.out
cargo test -p smix-authoring-ir >/tmp/c12ir.out 2>&1; echo "authoring-ir rc=$?"; grep "^test result:" /tmp/c12ir.out
cargo test -p smix-recorder >/tmp/c12rec.out 2>&1; echo "recorder rc=$?"; grep "^test result:" /tmp/c12rec.out
# 5. 无回归：clippy / hygiene / FFI bindings 不受改名影响
cargo clippy --workspace --all-targets >/tmp/c12clippy.out 2>&1; echo "clippy rc=$?"
python3 scripts/dev/hygiene-scan.py --noise-only >/dev/null 2>&1; echo "hygiene rc=$?"
bash scripts/dev/ffi-bindings-fresh.sh >/dev/null 2>&1; echo "bindings-fresh rc=$?"
```

期望，逐条：
1. `recorder-ir 残留=0`。
2. `dir OK`；`grep -c` 得 **1**（包名已改）；ship.sh 计数 **≥1**（发布清单已改）。
3. `SimctlError 残留=0`；`DeviceControlError=168`（承接原引用数，量级一致即可 —— 精确数以改名后实测为准，判据是**旧名 0 + 新名承接**）。
4. `build rc=0`；`cargo rc=0`；`test result: ok` 计数 **≥132**（前置条件实测基线，不回退）；`FAILED` 计数**与基线一致**（`smix-ai-tier` 6 个 stub-CLI 测试的偶发超时是既有环境 flaky，非本段回归 —— 见 `v2.md` 2026-07-18「C9 旁证」；若本次触发，基线对照 `/tmp/c12base.out`）；`smix-authoring-ir` 与 `smix-recorder` 各 `rc=0`、`test result: ok`。
5. clippy `rc=0`；hygiene `rc=0`；`bindings-fresh rc=0`（两个改名都不在 FFI 边界，此 gate 应原样通过 —— 若它红，说明改名意外碰到了 FFI，回查）。

**仪器纪律**（本 cycle 反复吃亏，逐条本次沿用）：
- 测退出码**不接管道** —— `cmd | head; echo $?` 量的是 `head`（本 cycle 已犯 ≥3 次，见 `perf-decomposition-vs-polish.md` §1 / `v2.md` C5、C7 记录）。rc 单独 `>/tmp/… 2>&1; echo "rc=$?"`。
- **计数用 `git grep -c` 逐文件再 `awk` 求和**，不靠肉眼；`git grep` 的 `-- 'crates/**/*.rs'` pathspec 由 git 解释，不受 zsh glob 摊开影响（不需引号问题，但仍显式带引号防意外）。
- **不在编译未完成时读测试输出** —— `exit=101 / 22 buckets` 是假读数（`v2.md` C7 实测）；落 `/tmp` 等命令整体结束再 grep。
- 第 1/3 组量的是**代码里的字符串计数**，`docs/`、`.claude/rfcs/`、根 `CHANGELOG.md` 的历史引用**故意不计**（改它们是 C13 docs 段的去消费者化编辑，非本段）。

**未被本 checkpoint 覆盖的**（写在明处，同 C3-C11 教训）：
1. **#1 / #3 / #4 三个 break 不在本段** —— 见文末拆分提案。C12 若被用户裁定为「四 break 一段」，则本 plan-hot 只是它的第一片，其余需追加 step（会超 §2 的 1-3 上限，故本段主张拆）。
2. **`cargo-semver-checks` 不在本段跑**（本机未装，是 C14 门禁）—— 本段只保证「旧名 0 + 新名承接 + build/test 绿」，两个 rename 是 major break 的**判定**归 C14。
3. **docs/roadmap 里的旧名指针同步**归 C13（roadmap 1.0.x sync + 死链清零本就是 C13 段）。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2-c12-hot.md`
2. 调 sub-agent 生成新 `plan-hot.md`（到 C13：#4 `Modifier(s)` 合并 + `openLink`/`openUrl` 模型 —— 拆分已按 §10 拍板，见 v2.md 2026-07-18「拍板·拆 C12」），见 CLAUDE.md §6
3. 在 `docs/v2.md` 决策日志补一行 §10：`SimctlError` → `DeviceControlError` 的命名拍板（理由：与 `DeviceControl` trait 同名族、两平台中性）。

## 与冷计划不符之处（必须先读，不要隐瞒）

**A. C12 太大，应拆 —— 提案在此，拍板属用户（§10 / CLAUDE.md §6）。** 冷计划 `plan-cold/v2.md:44` 把 C12 写成「四个改名/合并 break + `SimctlError`」一段。实测这是**四个互相独立、无共同 gate 的破坏性变更 + 一个 rename**，风险性质分三类，塞不进「1-3 step、线性」（§2）：

| 组 | 内容 | 风险性质 | 用户面 |
|---|---|---|---|
| **本段 C12** | #5 `recorder-ir`→`authoring-ir` + `SimctlError`→`DeviceControlError` | 机械改内部 Rust 符号/crate，零行为变化 | 仅 crates.io semver（C14 兜底），无 YAML codemod |
| 提案 C13new | **#4** `Modifier`(9-case sum type)合并进 `Modifiers`（Kotlin+Swift SDK）+ `openLink`/`openUrl` 模型 | 改**已发布 SDK 公开类型** + verb 模型决策 | SDK 公开 API break；`openUrl` 若并入需 YAML warn |
| 提案 C14new | **#1** sessions 强制去 Rust/CLI 隐式 no-session + **#3** `SMIX_*` 开关折进 config | 收紧**行为/表面** + 需 config-system 设计决策 | #3 是用户 env 面（codemod warn + config 生成）；#1 内部 API |

拆分判据与 C6/C7/C8/C9 完全同源（`v2.md` 多条「拆的理由不是工作量，是风险性质不同」）：**不像 C9 的 `route-conformance rc=0` 那样有一个只在全落后才成立的整体 gate** —— 这四项各自有独立的「旧名 0 / 新类型就位 / 行为改变」机器判据，天然可分。**未自行按此拆定**（checkpoint 边界属用户权力）；请用户裁：(a) 采纳三段拆分（本 plan-hot 即新 C12），下游 C13→C15、C14→C16 顺延；或 (b) 维持一段（则需接受 >3 step，与 §2 冲突，须显式豁免）。

**B. 冷计划把这五项都当「改名/合并」，但只有本段两项是纯改名。** 逐条实测对照 brief / 冷计划：
- **#4「`Modifier(s)` 重复只在 Kotlin+Swift SDK，Rust 只有 `Modifiers`，TS 无」—— 属实，且 C9/C10/C11 没碰它，重复**仍活着**。`android-runner/sdk/.../Modifier.kt`（9-case `sealed interface`：First/Last/Nth/Above/Below/LeftOf/RightOf/Near/Inside）+ `Modifiers.kt`（扁平 all-optional data class）并存；Swift 同构（`Modifier.swift` + `Modifiers.swift`）；Rust `smix-selector/src/lib.rs:220` 只有 `Modifiers`（+ `IndexModifiers`，是合法子集非重复）；`npm/smix-rn/src` 无 Modifier 文件。C9/C10/C11 只重塑了**驱动**代码、**保留** Selector/Modifier 类型面（C11 决策日志：「TS = 类型 + Selector + resolver seam」）。`Modifier.kt` 被 `MvpApiShapeTest.kt` 引用（公开 API + 形状测试）—— **合并是真活，涉两个已发布 SDK 公开类型**，非 moot。
- **#4「双 open_url」—— 与 brief 预期不同，需报明**：全仓没有两份 open_url **实现**在竞争。实测的「双」是 `VERB_TABLE` 里 `v("openLink","openUrl",…)`（`smix-verbs/src/lib.rs:308`）—— maestro 名 `openLink` → smix 名 `openUrl` 的**单行别名**；而 parser **只 dispatch `openLink`**（`parser.rs:2320` `"openLink" => parse_open_link`），`openUrl` 这个 smix 名**从不可解析**。即「双」是两个拼写（一个还接不上 parser），不是两套模型。**「合并单模型」到底指什么，需要 §10 决策**（把 `openUrl` 接上 parser？还是删掉这个从不生效的 smix 名？）—— 这正是它该和 #4 一起进 C13new 而非本段的原因。
- **#1「sessions 强制的另一半」—— 是内部 Rust 收紧，不是改名。** 实测：`HttpRunnerClient.session_id: Option<String>`（`smix-runner-client/src/lib.rs:372`，构造 `None`@431，`set_session_id`@546 / `clear_session_id`@554 / `session_id()->Option`@559）。需要 session 的方法在运行期查 `Option`、缺失时报带 hint 的错（`smix-sdk/src/lib.rs:978`「no session id on the client; run `smix run` … or call `App::open_session` first」）。`smix run` 自动开 session。**去隐式 no-session = 把 session 从「运行期 Option + 报错」提到类型层强制**，是 runner-client + smix-sdk App/Session + smix-cli 的内部重构，**不是改名，也无用户 YAML 面**（brief 的判断正确：可能比「改名」大）。
- **#3「`SMIX_*` 折进 config」—— 39 个里只 ~4-6 个是行为开关，且 config 系统『半存在且分裂』。** 实测 39 个 distinct `SMIX_*`（`git grep -oE "SMIX_[A-Z_]+" | sort -u | wc -l`）。真·行为开关（product 代码里 env 读）：`SMIX_AUTO_OCR_FALLBACK`（`parser.rs:79`）· `SMIX_ENABLE_AI_ASSERTIONS`（`parser.rs:110`，C2 明记「break #3 会把它和其余一起折进 config」）· `SMIX_ASSERT_SCREENSHOT_NO_AUTORECORD`（`smix-sdk/src/lib.rs:1623`）· `SMIX_LAUNCH_FRESH_FORCE_REINSTALL`（`smix-sdk/src/lib.rs:1103`）；边界项 `SMIX_TAP_OCR_POLL_MS`（`runtime.rs:3061`，调优值）· `SMIX_STD_SUBFLOWS`（`runtime.rs:3462`，路径覆盖）。其余是**运营**（`SMIX_RUNNER_PORT`/`_PROJECT`/`_TARGET_BUNDLE`/`SMIX_UDID`/`SMIX_BUNDLE_ID`/…，**不折**，brief 判断正确）或**测试专用**（`SMIX_APP_PATH_COM_*`/`SMIX_REAL_SIM_FLOW`/…）。**config 系统实测是碎的**：`.smix/config.yaml`（`interactiveProbe`，`cli/runner.rs:99` 读）**与** `.smix/config.json`（`metroLog`/`fixturesRegistry`，`runtime.rs` 多处作 hint 引用）**并存，无统一 loader** —— #3 得先消解 `.yaml` vs `.json` 的矛盾并建统一读法（**这本身是一次 §10 设计决策 + 中等体量工程**），brief 的「可能得建一套 config，较大」成立（半建，不是从零）。
- **`SimctlError` #10 文件 / 168 处 —— 与 brief/冷计划逐字吻合**（上文 §① 已列 per-file 分解）；无出入。
- **#5 recorder-ir —— 与冷计划吻合**，唯一需澄清：`smix-input` 的那处不是依赖、是文档注释提及（上文 §②）。

**C. 冷计划 C12 行没写「C13/C14 顺延」的下游影响。** 若采纳拆分，`plan-cold/v2.md` 的 Checkpoint 概览（C13 docs、C14 ship）需顺延为 C15/C16，`docs/v2.md:42` 的概览行同步 —— 归用户拍板后的 docs 更新，不在本段代码里改。
