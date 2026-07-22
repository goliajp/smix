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
| 1 | 围栏式 AI 断言层(`assertCondition` / `extractWithAI`,opt-in,firewall 在 sense 之外) | shipped | `at crates/smix-adapter-maestro/src/parser.rs:1637 "assertCondition"` | 独立 crate `smix-ai-tier` + parser 认这两个 verb;删该 crate 不动任何 sensing 代码,即其 fence 的证明 | 2026-07-22 |
| 2 | iOS + Android 全 parity 作发布门槛(达成 ✅ 或诚实 re-tier) | shipped | `at crates/smix-adapter-maestro/tests/parity_table_verbs_parse.rs:100 "every_verb_in_the_table_has_a_row_on_the_parity_page"` | 两向对账的测试:表上每个 verb 都解析得出、VERB_TABLE 里每个 verb 都在表上有行。非 ✅ 的行各带平台级理由(剪贴板归焦点应用 / KeyStore host 够不到 / `pm clear` 语义)。**闸门拒绝了我原先钉 `verb-parity.md` 的写法** —— 我当时的理由是"这一项的交付物就是那份表",而那正是 `--stable` 靠四份文档自洽七个月的同一句话 | 2026-07-22 |
| 3a | MCP 驱动面:fill / swipe / scroll / launch / stop / assert 六类 | shipped | `at crates/smix-mcp/src/main.rs:297 "open_session_in_place"` | 13 个 tool 覆盖这六类,各映射已有 `App::` 方法 | 2026-07-22 |
| 3b | MCP 驱动面:`session` 与 `diagnostic-dump` 两类 | pending | `none "async fn smix_diagnostic" in crates/smix-mcp/src/*.rs` | in-scope #3 逐个点名了八类,这两类没有 tool。`session` 可能根本不该是 tool(`smix_launch_app` 已 `open_session_in_place`,对外部 agent 已是隐式语义);`diagnostic-dump` 是薄包装(client 层 `diagnostic_dump` 已有,CLI 已在调)。材料见 scope-decisions-pending | 2026-07-22 |
| 4a | 确定性:真 animation-idle(frame-diff 取代固定 sleep) | shipped | `at crates/smix-adapter-maestro/src/parser.rs:1426 "ceiling_ms"` | `waitForAnimationToEnd` 三形式统一为"等到静止,上限 N ms";实测静止屏幕 ≈387ms 对 400ms 固定 sleep 持平,价值在正确性与大 ceiling | 2026-07-22 |
| 4b | 确定性:`--stable`(冻结动画 / 时间 / 抖动) | pending | `none "--stable" in crates/**/*.rs` | 全仓零实现(四种 grep 模式各查一次)。**追到源头是一份 `🔬 explored` 的探索记录**,且原文要求被测 app 侧配合 —— in-scope 把探索拔高成承诺并丢了那一半。对外从未承诺过。材料见 scope-decisions-pending | 2026-07-22 |
| 5 | 六项破坏性变更 + `smix migrate` codemod | shipped | `at crates/smix-migrate/src/lib.rs:227 "pub fn migrate"` | 六项均已落地(sessions 强制 / wire 协商 / env 折 config / 选择器模型合并 / crate 改名 / VERB_TABLE freeze);迁移列已于 C2 改成属实的说法 | 2026-07-22 |
| 6 | 代码 & 文档 hygiene(去开发噪声 / 五矛盾收敛 / 宪法同步) | shipped | `at scripts/dev/hygiene-scan.py:180 "cjk-comment"` | 闸门存在且接进 preflight / CI / ship 三处(ship 那处是 C2 补的) | 2026-07-22 |
| 7 | 面向 AI 的文档(`llms.txt` / `llms-full.txt` + MCP 设置指南 + 官网 IA) | shipped | `at scripts/dev/gen-llms.py:304 "def main"` | 两个文件由 VERB_TABLE + Selector 投影生成,`--check` 新鲜度闸门在三处。**官网 IA 那一半不在此判据内** —— 见下方说明 | 2026-07-22 |

## 这张表说不清的

- **第 7 项的「官网 IA」**与第 2 项的「全 parity」、第 3 项的「驱动面成熟」都含**程度词**。
  判据只能钉住可指的那一半(生成器存在、parity 表存在、六类 tool 存在),
  钉不住"成熟"或"IA 做好了"。闸门的 docstring 里明写了这一点 —— 它不假装是裁判。
- **未被 `--flag` / MCP 能力两条点名规则覆盖的子交付物**仍可能静默缺席。
  唯一的对策是人拆行,不是给闸门加一张手工豁免表(那又是一份没人维护的清单)。
