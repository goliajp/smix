# scope-evidence — v2 范围承诺的实证状态

`docs/v2.md` 的「做什么（in scope）」列了 7 项。这张表逐项回答**它现在到底存不存在**,
判据由 `scripts/dev/scope-promise-scan.py` 每次运行重新求值。

**最要紧的一条规则:`shipped` 行的判据不许指向 `docs/` 下的文件。**
一份文档不是"这东西被造出来了"的证据。`--stable` 就是靠四份互相自洽的文档
活了七个月而没有一行代码 —— 那四份里有三份还是 gitignored 的,clone 后根本不存在。

`pending` 不是停车位,是**被盯住的终局状态**:pending 行要求
`docs/scope-decisions-pending.md` 里有齐备的决策材料,**并且**它的 `none` 判据持续成立。
东西一旦被造出来,那条判据就失配,这一行必须动。
(这是 `audit-ledger.md` 不对称引文的镜像:那张表堵"已修却仍标未修",这张堵"已做却仍挂待拍板"。)

| # | 承诺 | 状态 | 判据 | 依据 | 核验日 |
|---|---|---|---|---|---|
| 1 | 围栏式 AI 断言层(`assertCondition` / `extractWithAI`,opt-in,firewall 在 sense 之外) | shipped | `at crates/smix-adapter-maestro/src/parser.rs:2691 "assertCondition"` | 独立 crate `smix-ai-tier` + parser 认这两个 verb;删该 crate 不动任何 sensing 代码,即其 fence 的证明 | 2026-07-22 |
| 2 | iOS + Android 全 parity 作发布门槛(达成 ✅ 或诚实 re-tier) | shipped | `at crates/smix-adapter-maestro/tests/parity_table_verbs_parse.rs:100 "every_verb_in_the_table_has_a_row_on_the_parity_page"` | 两向对账的测试:表上每个 verb 都解析得出、VERB_TABLE 里每个 verb 都在表上有行。非 ✅ 的行各带平台级理由(剪贴板归焦点应用 / KeyStore host 够不到 / `pm clear` 语义)。**闸门拒绝了我原先钉 `verb-parity.md` 的写法** —— 我当时的理由是"这一项的交付物就是那份表",而那正是 `--stable` 靠四份文档自洽七个月的同一句话 | 2026-07-22 |
| 3a | MCP 驱动面:fill / swipe / scroll / launch / stop / assert 六类 | shipped | `at crates/smix-mcp/src/main.rs:329 "open_session_in_place"` | 13 个 tool 覆盖这六类,各映射已有 `App::` 方法 | 2026-07-22 |
| 3b | MCP 驱动面:`session` 与 `diagnostic-dump` 两类 | pending | `none "async fn smix_diagnostic" in crates/smix-mcp/src/*.rs` | in-scope #3 逐个点名了八类,这两类没有 tool。`session` 可能根本不该是 tool(`smix_launch_app` 已 `open_session_in_place`,对外部 agent 已是隐式语义);`diagnostic-dump` 是薄包装(client 层 `diagnostic_dump` 已有,CLI 已在调)。材料见 scope-decisions-pending | 2026-07-22 |
| 4a | 确定性:真 animation-idle(frame-diff 取代固定 sleep) | shipped | `at crates/smix-adapter-maestro/src/parser.rs:1455 "ceiling_ms"` | `waitForAnimationToEnd` 三形式统一为"等到静止,上限 N ms";实测静止屏幕 ≈387ms 对 400ms 固定 sleep 持平,价值在正确性与大 ceiling | 2026-07-22 |
| 4b | 确定性:动画默认压低(原 `--stable`) | shipped | `at crates/smix-sdk/src/android_device.rs:219 "animation_settings_verified"` | **原承诺按重新设计交付,名字废掉**。`--stable` 承诺的是结果(稳),而结果 smix 验证不了,且原方案要被测 app 读 env 冻自己的时钟 —— 那一半 smix 无法单方面做到、也无法验证 app 有没有照做。改为承诺机制:动画默认压低,Android 三个 scale 归零、iOS Reduce Motion,**两者都回读校验**,`--animations` 恢复。时钟冻结明确不做,理由写进 in-scope #4。 | 2026-07-22 |
| 5 | 六项破坏性变更 + `smix migrate` codemod | shipped | `at crates/smix-migrate/src/lib.rs:227 "pub fn migrate"` | 六项均已落地(sessions 强制 / wire 协商 / env 折 config / 选择器模型合并 / crate 改名 / VERB_TABLE freeze);迁移列已于 C2 改成属实的说法 | 2026-07-22 |
| 6 | 代码 & 文档 hygiene(去开发噪声 / 五矛盾收敛 / 宪法同步) | shipped | `at scripts/dev/hygiene-scan.py:185 "cjk-comment"` | 闸门存在且接进 preflight / CI / ship 三处(ship 那处是 C2 补的) | 2026-07-22 |
| 7 | 面向 AI 的文档(`llms.txt` / `llms-full.txt` + MCP 设置指南 + 官网 IA) | shipped | `at scripts/dev/gen-llms.py:305 "def main"` | 两个文件由 VERB_TABLE + Selector 投影生成,`--check` 新鲜度闸门在三处。**官网 IA 那一半不在此判据内** —— 见下方说明 | 2026-07-22 |
| 8 | runtime 速度(进程内 runner cycle + 截图管线 hoist,decomposition-first) | shipped | `at crates/smix-simctl/src/screenshot_pacer.rs:70 "pub struct ScreenshotPacer"` | 截图管线 hoist 落地为独立 `ScreenshotPacer`(interval floor + backpressure circuit),配进程内 soft-cycle 免 xcodebuild respawn;v2.8-C1 | 2026-07-24 |
| 9 | 规模:`smix run --parallel N` 多 sim 编排(N=1 保持现契约) | shipped | `at crates/smix-cli/src/parallel.rs:76 "pub fn run_parallel"` | `shard_flows` 分片 + `run_parallel` 多 sim 编排,N=1 走原单机路径;v2.8-C3 | 2026-07-24 |
| 10 | CI 硬化:`smix bench` 回归检测(超 5% CI 挂)+ 分级压测台 | shipped | `at crates/smix-cli/src/bench.rs:23 "pub const TOLERANCE_PCT: f64 = 5.0"` | baseline 相对回归门,超 5% 报 regression 并非零退出,接进 CI;v2.10-C1 | 2026-07-24 |
| 11 | 平台补全:Android 运行时 parity(rate-limit pacer + app-alive cache)+ 遮挡感知 | shipped | `at android-runner/app/src/main/kotlin/dev/smix/runner/AppAliveCache.kt:23 "class AppAliveCache"` | Android 侧补齐 `AppAliveCache` + `ScreenshotPacer` 与 iOS parity,遮挡感知走 chain 判定;v2.11-C1 | 2026-07-24 |
| 12 | SDK 补全:TS 经 napi 驱动(跨 triple `.node`,退掉 not-implemented 桩) | shipped | `at crates/smix-node/src/lib.rs:42 "pub struct SmixNodeDriver"` | `SmixNodeDriver` 经 napi 边界真驱动 sim,TS SDK 从全抛桩改为真调,闭合四 SDK parity;v2.9-C5 | 2026-07-24 |
| 13 | authoring/编排:跨平台 recorder + LLM-in-loop authoring + run federation | shipped | `at crates/smix-cli/src/federation.rs:421 "pub fn run_federation"` | `run_federation` 分布式多节点编排(结果合并 + 制品回收),配 recorder tier 与 `propose_and_amend` LLM-in-loop;v2.12-C5 | 2026-07-24 |
| 14 | 文档:AI-authoring guide + SessionState playbook | shipped | `at crates/smix-error/tests/sdk_readme_api_exists.rs:27 "SESSIONS_GUIDE"` | SessionState playbook(`09-sessions.md`)由测试 include_str! 钉住 API 存在性,authoring guide 同套;v2.12-C5 | 2026-07-24 |

## 这张表说不清的

- **第 7 项的「官网 IA」**与第 2 项的「全 parity」、第 3 项的「驱动面成熟」都含**程度词**。
  判据只能钉住可指的那一半(生成器存在、parity 表存在、六类 tool 存在),
  钉不住"成熟"或"IA 做好了"。闸门的 docstring 里明写了这一点 —— 它不假装是裁判。
- **未被 `--flag` / MCP 能力两条点名规则覆盖的子交付物**仍可能静默缺席。
  唯一的对策是人拆行,不是给闸门加一张手工豁免表(那又是一份没人维护的清单)。
