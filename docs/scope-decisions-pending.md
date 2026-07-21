# scope-decisions-pending — 等你拍板的范围承诺

`docs/scope-evidence.md` 里状态为 `pending` 的每一项,在这里有一份材料。

**这份文件不含推荐。** 每项的最后一节只列三条路径各自的后果 ——
做 / 撤回 / 继续挂着。选哪条是委托方的判断,不是我的(§13)。

最后一项被拍板之后,这个文件应该消失。

---

## `--stable`(冻结动画 / 时间 / 抖动)

### 承诺原文与出处

`docs/v2.md` 「做什么（in scope）」第 4 条:

> **确定性** — `--stable`（冻结动画 / 时间 / 抖动）+ 真 animation-idle（frame-diff 取代固定 sleep，闭合 §9#4 最后一处）。

同条的后半(真 animation-idle)**已交付**,前半从未开始。

**追到源头**:`docs/dogfood-archive/insight-roadmap.md` §K,
标题写着 `Deterministic time / animations mode (P3, milestone v0.5.0, cost S)`,
状态 **`Status: 🔬 explored`**。那是一份**探索记录**,不是承诺:优先级 P3、
里程碑早于 v2、成本 S,而且原文列的三步里有一步**要求被测 app 侧配合**
—— in-scope #4 把它写进 v2 交付物时,把 app 侧那一半丢了。

另外三处提及都在 `.claude/design/v2.0/*.html`(`features.html` 标状态 `design`、
`roadmap.html` 列进里程碑、`index.html` 拿它反向论证「只支持模拟器」这条不变量)。
**这三份是 gitignored 的**(`.gitignore:15` 的 `.claude/*`),clone 之后不存在。

**对外从未承诺**:`CHANGELOG.md` / `README.md` / `llms.txt` / `llms-full.txt` /
`docs/roadmap.md` / `docs/ai-guide/` / `web/` / `dashboard/` / `npm/` 全部零命中。

### 实证现状

零实现,四种模式各查一次(防窄 grep 得出假否定 —— 07-21 踩过 `impl FlowAttemptShape` 的坑):

- 字面 `--stable`:`crates/` `swift-bridge/` `android-runner/` `npm/` **0 命中**
- 标识符族(`stable_mode` / `freeze_animation` / `slow_animations` / `reduce_motion` /
  `deterministic_mode`):只命中 `crates/smix-simctl/src/lib.rs` 的 `set_reduce_motion`
- clap 的 `long = "stable"`:**0 命中**
- `SMIX_STABLE`:实现树 **0 命中**

已有的半块砖:`set_reduce_motion` 在 simctl 层**已存在但零调用方**。
`status_bar override` 与 Swift 侧的 `setAnimationsEnabled` **都不存在**。

### 若要做:动哪几层

`cli`(新 flag)+ `sdk`/`driver`(把 flag 送下去)+ `swift-runner`(禁用动画、冻结时间)
+ **被测 app 侧**(源文档原文要求的那一半 —— 这一层不在 smix 里)。

关键判断点:源文档的三步是 `status_bar override` + runner 禁用动画 + `SMIX_STABLE=1`,
其中第三步要 app 自己读这个 env 并冻结它自己的动画/时钟。
**smix 单方面做不出"确定性"** —— 只能做到"smix 这一侧尽力"。
这也是它当初被标 `🔬 explored` 而不是 `planned` 的原因。

### 若要撤回:先例与草稿

先例是 `docs/v2.md` 决策日志 2026-07-17 撤回 OCR 键盘 fallback 那条,
它的形状是:**回源核对 → 指出 dossier 误读 → 说明底层问题是否已被别的东西解决 →
说明它本身是不是降级 → 说明是否让步 parity**。那条的教训原文写着
「dossier 里「已 defer / 已 flag」这类关于自家历史的断言,必须回源核对」——
本项是同一个物种第二次。

草稿:

> - 2026-XX-XX [撤回 `--stable`] in-scope #4 的前半从未实现,现正式撤回。
>   **前提是误读**:源头 `insight-roadmap.md` §K 状态为 `🔬 explored`、P3、
>   里程碑 v0.5.0,是探索记录不是承诺;in-scope 把它拔高成 v2 交付物,
>   并丢掉了原文要求的**被测 app 侧配合**那一步。
>   **smix 单方面做不出确定性** —— 没有 app 侧参与,只能做到"smix 这侧尽力",
>   而那不是这个 flag 承诺的东西。
>   **不让步任何 parity**(maestro 无对应能力)。**对外零承诺**,故无 deprecation 义务。
>   in-scope #4 的措辞同步改为只含已交付的 animation-idle。

### 不做也不撤回的代价

范围文件继续说 v2 要交付一个不存在的东西。**当下的具体代价**:
任何"v2 还差什么"的回答都要先绕过这一条;而它已经在本轮自我验收里
消耗过一次判断(先被记成"承诺了没做",追源后才知道是误读)。

它对外不可见,所以**不是用户可见的失信** —— 这一点让三条路径的紧迫性都不高。

### 需要你拍的板

- **做**:要接受"没有 app 侧配合就只能做一半",并决定那一半够不够叫 `--stable`。
- **撤回**:按上面草稿写进决策日志,并改 in-scope #4 的措辞。代价是 v2 的确定性
  只剩 animation-idle 一半。
- **继续挂着**:`scope-evidence.md` 保持 pending,闸门持续盯住"它没被偷偷做出来"。
  代价是范围文件继续与实现不符。

---

## MCP `diagnostic-dump`

### 承诺原文与出处

`docs/v2.md` 「做什么（in scope）」第 3 条逐个点名了八类能力:

> **MCP 驱动面成熟** — 6-tool MVP 扩到完整外部 agent 驱动面（fill/swipe/scroll/launch/stop/assert/session/**diagnostic-dump**）。

### 实证现状

13 个 tool 里没有它:`grep -c "async fn smix_[a-z_]*diagnostic" crates/smix-mcp/src/main.rs` = **0**。

但**下面几层都已就绪**:`crates/smix-runner-client/src/lib.rs:1113` 有
`pub async fn diagnostic_dump(...)`,`crates/smix-cli/src/main.rs:2313` 已经在调它
(`smix diagnostic dump`)。

### 若要做:动哪几层

`mcp` 一层。它是**薄包装**:加一个 `#[tool]` 方法转发到已有的 client 调用,
形状与既有 13 个 tool 同构。没有新 wire、没有新 runner 路由、没有 SDK 改动。

### 若要撤回:先例与草稿

先例同上(07-17 那条)。但**撤回的理由不成立**:这一项没有"前提是误读"的成分,
in-scope 点名它、下层也确实齐备,缺的只是最后一层包装。
若仍要撤回,理由只能是"外部 agent 不需要它",而那是产品判断,不是事实核对。

草稿:

> - 2026-XX-XX [撤回 MCP `diagnostic-dump` tool] in-scope #3 点名的八类里删掉这一类。
>   理由:外部 agent 拿 dump 做不了什么(它服务的是人做事后诊断),
>   而 `smix diagnostic dump` 已在 CLI 上。in-scope #3 措辞同步改。

### 不做也不撤回的代价

范围文件说驱动面"完整"而它少一类,且这一类的成本是所有未做项里最低的
—— 挂着的理由最弱。

### 需要你拍的板

- **做**:一层薄包装,与既有 tool 同构。
- **撤回**:要给出"外部 agent 不需要它"的产品判断,不能靠事实核对。
- **继续挂着**:代价同上,且它是三项里最容易被读成"拖着"的一项。

---

## MCP `session`

### 承诺原文与出处

同上,in-scope #3 的八类里点名了 `session`。

### 实证现状

没有 `smix_session*` tool(`grep -c` = **0**)。

但 **session 对外部 agent 已经是隐式语义**:`crates/smix-mcp/src/main.rs:297`
的 `smix_launch_app` 已经调 `app.open_session_in_place(&params.bundle_id, true)`,
其余 12 个 tool 的描述都写着 "Needs the session smix_launch_app opens"。

也就是说:**外部 agent 已经在 session 里操作了,只是它不需要自己管理 session。**

### 若要做:动哪几层

`mcp` 一层,但**要先回答它该长什么样**:
显式 `smix_open_session` / `smix_close_session` 会与 `smix_launch_app` 的隐式开启重叠,
外部 agent 会拿到两条开 session 的路径 —— 那是给 MCP 表面加歧义,不是加能力。
另一种形状是只加 `smix_session_state`(只读),那更接近 diagnostic 而不是 session 管理。

### 若要撤回:先例与草稿

草稿:

> - 2026-XX-XX [撤回 MCP `session` tool] in-scope #3 点名的八类里删掉这一类。
>   理由:**它已经以隐式形式交付** —— `smix_launch_app` 开 session,
>   其余 tool 在其中操作。显式 session tool 会与之重叠,给外部 agent 两条路径,
>   是加歧义不是加能力。in-scope #3 措辞改为说明 session 是隐式语义。

### 不做也不撤回的代价

同上,但这一项还多一层:**范围文件的点名让它看起来是缺的,而它其实是"以另一种形式在的"**
—— 读的人会去找一个不该存在的 tool。

### 需要你拍的板

- **做**:要先定形状(显式管理 vs 只读状态),且要处理与 `smix_launch_app` 的重叠。
- **撤回**:按草稿说明它是隐式交付的,措辞改成陈述这个事实。
- **继续挂着**:代价是范围文件持续暗示一个不该存在的 tool。
