# plan-hot — v2 到 C15：破坏性变更 #3 `SMIX_*` 开关折进 `.smix/config.yaml`

> **单 checkpoint 判定（先读）**：冷计划 C15 = break #3（六项破坏性变更的最后一项）。本段实测后判定 **C15 恰好装得下一个 checkpoint，不需再拆** —— 与 C6/C9/C12 三次拆分不同：那三次是「风险性质不同」（修坏的 vs 改能用的 / 多语言各自破坏面），而 C15 是**单一 Rust 关注点、风险均匀**（把 4 个开关从「散在 parse/run 两处的 `env::var` 直读」收敛到「CLI 一处 resolve + 注入」）。4 个开关全部流经 **同一个 resolver**（S1 产出），两处注入（parser 的 thread-local override 缝 + App 字段）机制虽异但风险同为「plumbing 一个已 resolve 的 bool」。**是否仍要拆成 loader+parser 注入 / sdk 注入两段属用户权力（§10）；本段 plan-of-record 取单 checkpoint。**

## 目标 checkpoint

C15：**4 个行为开关的唯一权威来源是 `.smix/config.yaml` 的 `switches:` 块，`SMIX_*` env 降级为「仍生效 + 具名 deprecation warn」的兼容层。** 通过后世界：`smix run` 启动时由 CLI 一处读 `switches:` 块、按 `config.yaml switches > SMIX_* env > 默认` 优先级 resolve 每个开关（用到 env 时打具名 warn「`SMIX_AUTO_OCR_FALLBACK` is deprecated; use `.smix/config.yaml` `switches.autoOcrFallback`」），resolve 结果**注入** parser（经既有 `set_auto_ocr_fallback_override` / `set_ai_assertions_override` thread-local 缝，parser **不获得文件 IO / workspace-root 依赖**，其 parse-时确定性契约不变）与 `smix-sdk::App`（两个新 `Option<bool>` 字段 + builder）；`parser.rs` 与 `smix-sdk` 的 `env::var` 直读保留为**非 CLI 调用方**（tests / 库直用）的 None-fallback，不再是 `smix run` 的路径；所有 `.smix/config.json` 虚构 hint 串改指 `.smix/config.yaml`。

## 前置条件

```bash
git branch --show-current                                    # feature/v2.0
git status --short | grep -c .                               # 期望 0（干净树）
test -f docs/plan-history/v2-c14-hot.md && echo "C14 archived"   # 已归档
test ! -e docs/plan-hot.md && echo "no stale hot plan"           # C14 已 mv
# in-house batch / gradle 不活动（本段只动 Rust，不起 emulator/sim，但守规程）
pgrep -fl "runner.ts|smix run|supervise|bun test:e2e" ; echo "batch rc=$?"
```

基线测试数（**取自 C14 close 决策日志实测，入场须复跑复核，不得凭记忆**）：cargo `133 ok / 0 failed`（`smix-ai-tier` 6 个 stub-CLI 测试偶发超时，非本段回归，见 v2.md 2026-07-18 C9 旁证）、swift `319/0`、route `rc=0`、clippy/hygiene/bindings-fresh `rc=0`。

## 已确证的起点（本次热化实测，file:line，非转述）

**4 个开关的确切读点（grep 复核，其余同名命中均为 doc/error 文案，非读）：**

| 开关 | 读点 | 时机 | 缝 |
|---|---|---|---|
| `SMIX_AUTO_OCR_FALLBACK` | `smix-adapter-maestro/src/parser.rs:79`（`auto_ocr_fallback_enabled`） | **parse 时** | thread-local override `set_auto_ocr_fallback_override`（parser.rs:70，`#[doc(hidden)]`） |
| `SMIX_ENABLE_AI_ASSERTIONS` | `parser.rs:110`（`ai_assertions_enabled`） | **parse 时** | thread-local override `set_ai_assertions_override`（parser.rs:101，`#[doc(hidden)]`） |
| `SMIX_ASSERT_SCREENSHOT_NO_AUTORECORD` | `smix-sdk/src/lib.rs:1678`（`App::assert_screenshot`，`env::var_os`） | **run 时** | 无 |
| `SMIX_LAUNCH_FRESH_FORCE_REINSTALL` | `smix-sdk/src/lib.rs:1158`（`App::launch_fresh`，`env::var`） | **run 时** | 无 |

**注入架构（本段最关键的一件事，已按调用图核）—— loader 在 CLI，无需新 crate：**

- **调用图**：`smix-cli → smix-adapter-maestro → smix-sdk`（`smix-cli/Cargo.toml` 亦直依赖 `smix-sdk`）。`smix-sdk` **对 config 一无所知** —— grep 实测其 `src/` 无 `workspace_root` / `current_dir` / `.smix/` / `config.yaml` / `serde_norway`（依赖表只有 screen/selector/input/error/runner-client/driver/simctl/adb），故它**不能自读 config**，值必须被注入。`smix-sdk` 亦**不能**反向依赖 `smix-cli`（会成环）。
- **唯一现存真 reader 在 CLI**：`smix-cli/src/runner.rs:97` `load_interactive_probe_env` 走 `workspace_root(cwd)`（runner.rs:108）→ 读 `.smix/config.yaml` → `serde_norway` → `serde_json::Value`（故意 schemaless）。**loader 就该长在这里**（同文件加 `load_switches`），因为它已同时满足：(a) 知 workspace root、(b) 已在读 config.yaml、(c) `main.rs` 是 `run_flow`/App 构造的编排点，注入在此发生。**不需要新 config crate** —— 只有 CLI 读 config，parser/sdk 只收注入。
- **`smix run` 的 parse 发生在 `run_flow` 内部**：`main.rs:1366` 调 `smix_adapter_maestro::run_flow(FlowArgs{..})`；`entry.rs:126` `run_flow` 在 :219 调 `parse_flow_file`。**关键**：:219 之前有 `.await`（:193 `open_session`、:212 `foreground`），tokio 多线程 runtime 可能在 await 处迁移 worker 线程 —— 故 thread-local override **必须紧贴 :219 之前设、其间无 await**（parse 是同步的，紧随其后）。App 在 :129/:130 构造、:142 `configure` 闭包链式 `.with_udid/.with_bundle_id/...` —— **两个 run-时开关的注入点就在这个 builder 链里**。
- **第二个 parse 入口 = `--check`**：`main.rs:1263` `parse_flow_yaml`（同步、无 tokio、`main.rs` 知 workspace root）。它也须在 parse 前设两个 parse-时 override，才与 `smix run` 的 parse 一致。

**`.smix/config.json` 从不被读（决策日志已记，本次复核）**：全仓唯一 `fs::read_to_string(config)` 是 runner.rs:100 读 **config.yaml**；9 处 `config.json` 命中全在 hint/doc 串（`main.rs:367,370` · `smix-metro-log/src/lib.rs:249` · `smix-fixture/src/lib.rs:14` · `runtime.rs:857,868,2411,2427,2497`），无 loader。`metroLog`/`fixturesRegistry` 的真实来源是 `--metro-log-url` flag / `fixture_registry` 结构字段。

**已锁定的 3 个 config 决策（2026-07-18 用户拍板，不再 re-litigate）**：(a) 统一到 config.yaml `switches:` 块，config.json 虚构 hint 全改指 yaml；(b) env 保留 + 具名 deprecation warn，优先级 `config.yaml switches > SMIX_* env > 默认`；(c) schema = `switches:` 块、保留 yaml schemaless、保留 parser 两个 thread-local 测试缝。

## 决策 C15 落地形态（§10 —— 3 个锁定决策的实现选择，动手时若与实测冲突须回报）

- **D-a〔loader + resolver 归口 CLI，无新 crate〕**：`runner.rs` 加 `load_switches() -> SwitchesConfig`（4 个 `Option<bool>`，None = key 缺）；CLI 加 `resolve_switch(config: Option<bool>, env_name: &str) -> (bool, source)`，`source` 携带「值来自 config / env / 默认」以便 CLI 打具名 warn 且测试可断言（warn 不进 parser/sdk —— 它们是纯逻辑/设备层，warn 属 CLI 边界）。
- **D-b〔注入而非机械 env→file 替换〕**：parser 两开关经既有 thread-local 缝注入（production 首次调这两个 `#[doc(hidden)]` 缝，其「production 从不调用」注释同步改真）；sdk 两开关经 App 新字段注入。**parser/sdk 的 `env::var` 直读保留为 None-fallback**（非 CLI 调用方 = tests / 库直用 / MCP 仍走 env，符合决策 (b)「`SMIX_*` 仍生效」；`smix run` 的 env 语义则由 CLI resolver 统一承载并打 warn）。
- **D-c〔schema 保持 schemaless〕**：`load_switches` 复用 `serde_norway → serde_json::Value`，逐 key `.get("autoOcrFallback").and_then(Value::as_bool)`，不引入 struct schema（决策 (c)）。

## 步骤（线性，无分叉）

### S1. `.smix/config.yaml` `switches:` 读取 + 优先级 resolver（config > env > 默认 + 具名 warn）

**红（写测试）**
- 文件：`crates/smix-cli/src/runner.rs`（`#[cfg(test)]`，随现有 `workspace_root` 测试）
- 断言：① 写临时 `.smix/config.yaml` 含 `switches: { autoOcrFallback: true }` → `load_switches()` 返回 `auto_ocr_fallback == Some(true)`、其余三项 `None`；② `resolve_switch(Some(false), "SMIX_AUTO_OCR_FALLBACK")` 在 env 设为 `1` 时仍得 `false`（config 赢，source=Config，**不打 warn**）；③ `resolve_switch(None, name)` 在 env 设为 `1` 时得 `true` 且 `source=Env`（→ CLI 会打具名 warn）；④ `resolve_switch(None, name)` env 未设 → `false`、`source=Default`。当前红：`load_switches` / `resolve_switch` 不存在。

**绿（实现）**
- 文件：`crates/smix-cli/src/runner.rs`
- API：`pub fn load_switches() -> SwitchesConfig`（镜像 `load_interactive_probe_env`：`workspace_root(cwd)` → 读 config.yaml → `serde_norway` Value → 逐 key `as_bool`，schemaless）；`pub fn resolve_switch(config: Option<bool>, env_name: &str) -> ResolvedSwitch`（`ResolvedSwitch { value: bool, source: SwitchSource }`，`SwitchSource::{Config,Env,Default}`）。
- 关键点：resolver 是**唯一**读这 4 个 `SMIX_*` env 名的 production `smix run` 路径；warn 文案在 CLI 由 `source == Env` 触发（S2 接线）。

**重构**
- 若 `load_interactive_probe_env` 与 `load_switches` 重复了「walk root + read yaml + serde_norway」前半段，抽一个 `read_config_yaml() -> Option<serde_json::Value>` 共用（interactiveProbe 与 switches 同读一份文件、只取不同 key）。

### S2. 注入：4 个开关端到端由 resolve 结果驱动（parser 缝 + App 字段）

**红（写测试）**
- 文件：`crates/smix-adapter-maestro/tests/`（新，或加既有 parser/entry 测试文件）
- 断言（parser 侧）：设 `set_auto_ocr_fallback_override(Some(true))` 后 parse 一个裸字符串 selector → 得 `Fallback`（含 OCR），证「注入的 override 驱动 parse 形状」；且 override 复位 `None` 后回落 env（缝语义不变）。
- 文件：`crates/smix-sdk/tests/`（新）
- 断言（sdk 侧）：`App` 经新 builder `.with_assert_screenshot_strict(Some(true))` → `assert_screenshot` 在无 baseline 时走 strict（返 `DriverError`）**无视 env**；`.with_launch_fresh_force_reinstall(Some(true))` 同理驱动 reinstall 路径。当前红：字段/builder 不存在，方法只读 env。
- 文件：`crates/smix-cli/`（测试或 grep 断言）：`smix run` 构造 `FlowArgs` 时填入 resolve 结果、且 `--check` 路径在 parse 前设两个 parse-时 override。

**绿（实现）**
- 文件：`crates/smix-adapter-maestro/src/entry.rs`
  - `FlowArgs` 加 4 个 `pub` 字段：`auto_ocr_fallback: Option<bool>` / `ai_assertions: Option<bool>` / `assert_screenshot_no_autorecord: Option<bool>` / `launch_fresh_force_reinstall: Option<bool>`（`Option` = None 表「CLI 未注入，走既有 env fallback」；`smix run` 恒填 `Some(resolved)`）。
  - `run_flow`：在 :219 `parse_flow_file` **紧前**（其间无 await）调 `set_auto_ocr_fallback_override(args.auto_ocr_fallback)` / `set_ai_assertions_override(args.ai_assertions)`；`configure` 闭包（:142）链上加 `.with_assert_screenshot_strict(args.assert_screenshot_no_autorecord)` / `.with_launch_fresh_force_reinstall(args.launch_fresh_force_reinstall)`。
- 文件：`crates/smix-sdk/src/lib.rs`
  - `App` 加两个 `Option<bool>` 字段 + builder `with_assert_screenshot_strict` / `with_launch_fresh_force_reinstall`；`assert_screenshot`（:1678）/`launch_fresh`（:1158）改为「字段 `Some` 用之，`None` 回落既有 `env::var`」。
- 文件：`crates/smix-cli/src/main.rs`
  - run 路径（:1366 附近）：`let sw = runner::load_switches();` → 对 4 项 `resolve_switch`，`source==Env` 时 `eprintln!` 具名 deprecation warn → 填入 `FlowArgs` 四字段；`--check` 路径（:1263 前）：对两个 parse-时开关 resolve 后 `set_auto_ocr_fallback_override(Some(..))` / `set_ai_assertions_override(Some(..))` 再调 `parse_flow_yaml`。
- 关键点：注入在**一处**落（run_flow + 两 parse 入口），不逐调用方打补丁（§12.2）；parser 仍零 IO、零 workspace-root。

**重构**
- parser.rs 两个 `#[doc(hidden)]` override 缝的「production code paths never call this」注释改真（现在 run 入口注入 resolved config 经此缝）；不「顺便」碰 interactiveProbe 或其它 `SMIX_*` 运营项（§8.1）。

### S3. `.smix/config.json` 虚构 hint 串改指 `.smix/config.yaml`（决策 (a)）

**红（写测试）**
- 文件：`crates/smix-cli/tests/`（新 gate，或并入 hygiene）
- 断言：shipped Rust 源（`git ls-files 'crates/**/*.rs'`，排除 `tests/` 与本 gate 自身）中 `.smix/config.json` 命中数 == 0。当前红：9 处。

**绿（实现）**
- 文件：`main.rs:367,370` · `smix-metro-log/src/lib.rs:249` · `smix-fixture/src/lib.rs:14` · `runtime.rs:857,868,2411,2427,2497`
- 动作：9 处 `.smix/config.json` → `.smix/config.yaml`（决策 (a) 锁定「虚构 hint 全改指 yaml」）。

**重构**
- 无（纯串替换）。

## Checkpoint C15 验收

```bash
# 1. switches loader + resolver 存在且优先级/warn 正确（S1）
cargo test -p smix-cli 2>&1 | grep "^test result:" | tail -3
# 2. parser 两个 thread-local 测试缝仍在（决策 c 要求保留）
grep -c "pub fn set_auto_ocr_fallback_override\|pub fn set_ai_assertions_override" crates/smix-adapter-maestro/src/parser.rs   # 期望 2
# 3. 注入端到端（S2：parser override 驱动 parse 形状 + App 字段驱动 sdk 开关）
cargo test -p smix-adapter-maestro --test '*' 2>&1 | grep "^test result:" | tail -5
cargo test -p smix-sdk 2>&1 | grep "^test result:" | tail -3
# 4. 4 个开关的 env 直读不再是 smix run 路径的唯一来源（FlowArgs 有四字段 + main.rs 走 load_switches）
grep -c "load_switches" crates/smix-cli/src/main.rs   # 期望 ≥1
grep -c "auto_ocr_fallback\|ai_assertions\|assert_screenshot_no_autorecord\|launch_fresh_force_reinstall" crates/smix-adapter-maestro/src/entry.rs   # 期望 ≥4
# 5. config.json 虚构 hint 清零（S3）
git ls-files 'crates/**/*.rs' | grep -v '/tests/' | xargs grep -l '\.smix/config\.json' 2>/dev/null | grep -vc 'config_json_hints' ; echo "json-hints rc=$?"
# 6. 无回归（rc 单独取，不接管道）
cargo test --workspace >/tmp/c15.out 2>&1; echo "cargo rc=$?"
grep -c "^test result: ok" /tmp/c15.out; grep -c "^test result: FAILED" /tmp/c15.out
cargo clippy --workspace --all-targets >/dev/null 2>&1; echo "clippy rc=$?"
python3 scripts/dev/hygiene-scan.py --noise-only >/dev/null 2>&1; echo "hygiene rc=$?"
python3 scripts/dev/route-conformance.py >/dev/null 2>&1; echo "route rc=$?"
bash scripts/dev/ffi-bindings-fresh.sh >/dev/null 2>&1; echo "bindings-fresh rc=$?"
```

期望，逐条：
1. `test result: ok`、`0 failed`（含 S1 loader/resolver 测试）。
2. 计数 **2**（两个测试缝定义原样保留）。
3. 两行均 `test result: ok`、`0 failed`（含 S2 注入测试）。
4. `load_switches` 计数 **≥1**；entry.rs 四字段命中 **≥4**。
5. `.smix/config.json` 在 shipped `.rs`（排除 tests）中命中文件数 **0**。
6. cargo `rc=0`、`ok` **≥133**（基线 + 新测试）、`FAILED` 与基线一致（`smix-ai-tier` 6 stub 偶发超时非本段，见 v2.md C9 旁证）；clippy/hygiene/route/bindings-fresh **rc=0**（route rc=0 = SDK 手术收口不回退；本段不碰 wire/route/FFI 边界，理应不动）。swift 本段无改动，不复跑。

**仪器纪律**（本 cycle 反复吃亏；每条都是 v2.md 决策日志记过的实伤）：
- **测退出码不接管道** —— `cmd | head; echo $?` 量的是 `head`（本 cycle 3+ 次）。rc 单独 `>/dev/null 2>&1; echo "rc=$?"` 或落 `/tmp`。
- **不在编译未完成时读测试输出** —— 曾读到假的 `exit=101 / 22 buckets`，真值 133/0。
- **绿 ≠ 已测**：数从 `test result:` 报告取，不估。
- glob 必带引号（`'crates/**/*.rs'`），否则 zsh `no matches found` 整条不执行。
- `gate`/`grep -c` 报的是「排版/命中」不是「工作」—— 确认量的是真物（S1 的 resolver 行为、S2 的注入生效），不是字符串数目对齐。

**未被本 checkpoint 覆盖的**（写在明处）：
1. **无真设备证据** —— S1/S2 全 mock / 单元；「config.yaml 开关在真 sim 上改变 `smix run` 行为」的端到端属 C17 ship gate（本 cycle C3/C4/C5 同一教训：mock 证明不了真设备）。
2. **`metroLog` / `fixturesRegistry` 仍无 config.yaml reader** —— S3 只按锁定决策把 hint 串 config.json→config.yaml，但这两个 feature 的真实来源仍是 CLI flag / 结构字段（决策日志已记 config.json 从不被读）。**repoint 让文件名说真话（config.yaml 是唯一真 config 文件），但「hint 承诺了一个 yaml 机制而 `metroLog`/`fixturesRegistry` 无 yaml loader」这层旧账 C15 不消除** —— 见「与冷计划不符」#3，属 flagged 观察。
3. **MCP / 库直用仍走 env** —— 4 个开关的 env 直读作为 None-fallback 保留（决策 (b)「`SMIX_*` 仍生效」），只有 `smix run` / `--check` 经 CLI resolver 承载 config 优先级 + 具名 warn。这是**有意**的兼容层，不是遗漏。
4. **`cargo-semver-checks`**（证公开 API 变化，FlowArgs +4 字段 / App +2 builder 均为增量非破坏）本机未装，属 C17 ship gate。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2-c15-hot.md`
2. 调 sub-agent 生成新 `plan-hot.md`（到 **C16：docs 重构 + `llms.txt`/`llms-full.txt` 生成 + 宪法/roadmap 同步 + 死链清零**，见 CLAUDE.md §6）。**前提**：六项破坏性变更（#1–#6）+ `SimctlError` 改名至此全部落地，v2 进入 docs/ship 收尾轨（C16/C17）。

## 与冷计划不符之处（必须先读，不要隐瞒）

1. **冷计划 C15 行的「建统一 config loader」不该读成「新建 config 子系统 / 新 crate」** —— 实测 `smix-sdk` 对 config 一无所知（无 workspace-root / 无 IO），而它**不读 config、只收注入**，故**不需要 CLI+sdk 共用的新 config crate**。loader 就长在 `smix-cli/src/runner.rs`（唯一现存真 reader + 已知 workspace root + run 编排点）。这比冷计划设想的轻。
2. **冷计划的「消解 json/yaml 裂缝」是半虚构** —— `.smix/config.json` 从不被任何代码读取（9 处全是 hint/doc 串，唯一真 reader 是 config.yaml）。故不存在两个 reader 要「消解」，S3 只是把虚构 hint 的文件名改真（决策 (a) 锁定）。冷计划 C15 注已记此点，此处复核确认。
3. **`metroLog`/`fixturesRegistry` hint 的旧账不随 config.json→config.yaml 消失** —— 这两个 feature 的真来源是 `--metro-log-url` flag / `fixture_registry` 字段，无任何 config 文件 loader。repoint 到 config.yaml 让文件名指向唯一真 config 文件，但「hint 承诺一个 config-文件机制而它俩没有 yaml loader」这层「注释是主张」仍在。**C15 按锁定决策 (a) 只 repoint 文件名**；是否进一步把这两个 hint 改指真实 CLI flag、或给它俩补 config.yaml reader，属 C16 docs 轨或独立决策，**flagged 给用户**（§10），本段不擅自扩范围（§8.1）。
4. **C15 = 单 checkpoint，不需再拆** —— 冷计划 C15 是单行 scope，本段实测判定它单一 Rust 关注点、风险均匀（4 开关同经一个 resolver），装得下一个 checkpoint（3 step 线性）。**与 C6/C9/C12 的拆分判据（风险性质不同）不匹配 → 不拆**。是否仍拆成 loader+parser 注入 / sdk 注入两段属用户权力，未自行拆。
