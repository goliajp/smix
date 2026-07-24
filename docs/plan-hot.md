# plan-hot — v2.11 到 C1:LLM-in-loop 回路可得性研究(observation→local-claude→actionable-proposal)

## 目标 checkpoint

C1:**read-only 研究先行**（decomposition-before-attack）。回答 v2.11 的机制不确定性 ——
**「smix runtime 观察一次 flow 执行的结构化记录,经本机 `claude` CLI,能不能产出可机械校验的
改进提议(actionable proposal),诚实的形是什么?」** 通过后世界变成:`docs/research/c1-llm-authoring-loop.md`
存在,含**先于证据钉死**的三轴证伪 rubric（观察面 / proposal 形 / 验证）+ file:line 级证据 +
`VERDICT: OBTAINABLE|NOT-OBTAINABLE|PARTIAL`。verdict 出后由用户/上层据其热化 C2（建造）或 re-tier。

**为什么是研究而非直接实现**:观察面**已现成**（`--debug-output` bundle + `--format json` +
`ExpectationFailure`,见前置条件实证),但 **proposal 形无任何现成 schema**、**验证无人读图的策略未定**
（well-formed device-free vs effective device-replay）—— 选错 proposal schema / 验证策略要废多个
实现 commit,正是 decomposition-before-attack 场景。研究把「proposal 的诚实形 + 验证分层」拆清,
再据 verdict 动手。**全程 read-only**:读源码 / 读 ai-tier 先例 / 试跑 `--debug-output` 看真产物,
**不写任何 proposal schema / 验证实现代码**。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix
grep -q 'pub struct StepDebugRecord' crates/smix-adapter-maestro/src/runtime.rs  # 观察面:per-step 结构化记录
grep -q 'debug_output' crates/smix-adapter-maestro/src/entry.rs                  # --debug-output bundle 面
grep -q 'pub struct ExpectationFailure' crates/smix-error/src/lib.rs             # 失败面 AI-readable
grep -q 'pub async fn judge' crates/smix-ai-tier/src/lib.rs                      # local-claude fenced 先例(decomp 对手)
grep -q 'pub fn suggest_selectors' crates/smix-cli/src/authoring.rs              # 现成 authoring 面(邻接,非 propose)
which claude                                                                     # 本机 claude CLI 在(§9#2)
```

全部 exit 0 = 可开工。任一失败 → 按 §6「何时该拒绝热化」回报,不硬开。

## 已经查清、不必重查的事实（planning 期已探测,C1 直接引用为证据起点）

- **观察面现成且结构化**:`smix run --debug-output <dir>`（`entry.rs` `FlowArgs.debug_output`)产
  `run-summary.json`（`runtime.rs:677` `StepDebugRecord`:n / verb / summary / verdict∈{ok,skipped,expanded-subflow,failed} /
  wall_ms / json_path / png_path / tree_path / failure_kind / failure_message）+ per-step `step-<N>-<verb>.json`
  + 失败 a11y tree JSON + fail-annotated PNG;`--format json`（`entry.rs:32` `OutputFormat::Json`)产顶层 run report
  + terminal `ExpectationFailure`（`smix-error/src/lib.rs:72`:visibleElements / suggestions / screenshot base64 /
  deviceLog / hint / selector）。
- **fenced 先例 = decomp「对手」**:`smix-ai-tier`（`judge` @ `lib.rs`)已证 screenshot+condition → local claude
  （`--tools Read -p <prompt> --output-format text` @ `lib.rs:138`)→ `StructuredVerdict{pass,reason}`;fenced（README:
  deletable test / opt-in / non-deterministic,坐 resolver 旁不在其内）。v2.11 propose = 这条回路的**新实例**:
  更富输入（bundle 而非单帧）、更富输出（proposal 而非 bool）。
- **proposal 侧无现成基础设施**:全 workspace 无 `propose` / `improve_flow` / `self-heal`;`authoring.rs` 有
  `suggest_selectors`（tree→SelectorCandidate）/ `diff_a11y_trees` 是**邻接**面,非「从一次 run 提议 flow 改进」。
- 本机 claude = 2.1.218。

## 步骤（线性,1 个;研究 checkpoint 的红/绿 = rubric 先于证据）

### S1. read-only decomposition:钉死三轴 rubric → 填 file:line 证据 → 落 VERDICT

**红（rubric 先于证据）**
- 文件:`docs/research/c1-llm-authoring-loop.md`
- 断言:先写下**三轴证伪 rubric**,每轴的 `OBTAINABLE` / `NOT-OBTAINABLE` **充分证据条件此刻钉死**,
  `Evidence:` 槽**留空**,**尚无 `VERDICT` 行**（证明 rubric 非事后合理化,同 c7-zorder 范式）:
  - **轴 A（观察面）**:`OBTAINABLE-A` iff 一次 flow run **无需新 core 能力**即产结构化、LLM-可消费记录
    （`--debug-output` bundle + `ExpectationFailure`),且携带 proposal 所需定位（失败步 index/verb/selector +
    失败时 a11y tree + visibleElements/suggestions）。`NOT-OBTAINABLE-A` iff 记录不结构化或不携带定位。
  - **轴 B（proposal 形）**:穷尽枚举 improvement 类（selector swap / waitFor 插入 / step reorder / 断言改 /
    verb 改）,逐类:`OBTAINABLE` iff 可表达为结构化、**可重新应用到 flow** 的编辑（落合法 `Step`/`Selector`）;
    `PARTIAL` iff 可表达但有损;`NOT` iff 无结构表达。（no-ceiling-words:负向结论须附穷尽枚举。）
  - **轴 C（验证,分两层）**:`well-formedness`（device-free:proposal 反解为合法 flow,现有 parser/IR 接受)
    `OBTAINABLE` iff 现成 parser 能校验;`effectiveness`（device replay:amended flow 从 fail→pass)
    `OBTAINABLE` iff 重跑回路无需新能力（`smix run` 现成）。
  - **Overall VERDICT 判定**:`OBTAINABLE`（三轴皆可 + ≥1 improvement 类端到端可得）| `PARTIAL`（部分改进类
    可得或验证只到 well-formed）| `NOT-OBTAINABLE`（穷尽枚举后无可得回路）。
- 跑红（须先失败一次,证明 verdict 尚未产）:
  ```bash
  test -f docs/research/c1-llm-authoring-loop.md \
    && ! grep -qE 'VERDICT: (OBTAINABLE|NOT-OBTAINABLE|PARTIAL)' docs/research/c1-llm-authoring-loop.md \
    && echo RUBRIC-FIRST-OK
  ```
  期望:打印 `RUBRIC-FIRST-OK`（rubric 在、verdict 未落 = 红）。

**绿（read-only 填证据 + 落 verdict）**
- 派 **read-only decomposition sub-agent**（Read + Bash + Grep,**无 edit 实现代码权限**;可试跑
  `smix run --debug-output` 看真产物、读 `runtime.rs`/`smix-error`/`smix-ai-tier`/`smix-adapter-maestro` parser 面）。
  产出:回填每轴 `Evidence:`（file:line 级,claim 有本机出处,不脑补）+ 落 `VERDICT:` 行 + Top-N「若 OBTAINABLE
  下一步建造的 attack 候选」（proposal schema 形 + 验证分层落哪 crate,给 C2 起点,不实施）。
- 关键点:①「对手」= `smix-ai-tier` 已证回路,proposal 回路逐段 side-by-side 拆（输入面 / claude 调用 / 输出解析 /
  fence 归位）;②轴 B 负向须穷尽枚举 improvement 类,不许「结构性拿不到」hand-wave（no-ceiling-words）;
  ③verdict 诚实 —— PARTIAL/NOT 都是合法答案,不为「像能做」凑 OBTAINABLE;④§9#2 只考虑本机 claude,网络路径不评。
- 跑绿:下方 Checkpoint 验收全绿。

**重构**
- 无（研究文档,无代码结构）。

## Checkpoint C1 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix
# —— 研究文档存在 + 含 verdict ——
test -f docs/research/c1-llm-authoring-loop.md \
  && grep -qE '^VERDICT: (OBTAINABLE|NOT-OBTAINABLE|PARTIAL)' docs/research/c1-llm-authoring-loop.md \
  && grep -qi 'Axis\|轴 A\|观察面' docs/research/c1-llm-authoring-loop.md \
  && grep -qi 'proposal\|轴 B' docs/research/c1-llm-authoring-loop.md \
  && grep -qi 'well-formed\|effective\|轴 C\|验证' docs/research/c1-llm-authoring-loop.md \
  && echo DOC-VERDICT-OK
# —— 关键 claim 有本机证据支撑（文档引的观察面 / 先例真实存在）——
grep -q 'pub struct StepDebugRecord' crates/smix-adapter-maestro/src/runtime.rs \
  && grep -q 'pub struct ExpectationFailure' crates/smix-error/src/lib.rs \
  && grep -q 'pub async fn judge' crates/smix-ai-tier/src/lib.rs \
  && echo EVIDENCE-ANCHORS-OK
```

期望:两行 `DOC-VERDICT-OK` + `EVIDENCE-ANCHORS-OK` 均打印,各命令 exit 0。含义 =
研究文档存在、含三轴 rubric + 明确 `VERDICT`,且文档所依赖的观察面（StepDebugRecord / ExpectationFailure）
与 fenced 先例（ai-tier judge）在本机真实存在（claim 非脑补）。

**不在 C1 验收内（诚实划界）**:任何 proposal schema / 验证实现代码（verdict = OBTAINABLE 后归 C2+）;
真调 claude 产 proposal 的端到端（属 C2 生成核心）;device replay 有效性（属 C4）。C1 只交 verdict 文档。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.11-c1-hot.md`。
2. C1 verdict 写入 `docs/v2.md` 决策日志一行（回路可得性结论 + 若 re-tier 的概要调整）。
3. **由用户/上层据 verdict 拍板**:`OBTAINABLE/PARTIAL` → 调 sub-agent 热化 C2（proposal schema + 生成核心,守
   §9#2 本机 claude + §9#8 三层 fenced 归位）,见 CLAUDE.md §6;`NOT-OBTAINABLE` → 据 verdict re-tier v2.11
   概要列表,进决策日志,不硬凑。
4. **§9#2 网络路径决策点**:若用户此时想放开直连 Claude API（非本机 CLI）,属独立不变量修订 —— 单独拍板进决策日志,
   不在 C1/C2 偷改。发布顺延待用户授权,不自作主张 publish。
