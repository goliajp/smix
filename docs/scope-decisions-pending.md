# scope-decisions-pending — 等你拍板的范围承诺

`docs/scope-evidence.md` 里状态为 `pending` 的每一项,在这里有一份材料。

**已拍板并移出本文件**:`--stable` —— 2026-07-22 按重新设计交付(动画默认压低,名字废掉),
见 `docs/v2.md` 决策日志 [v2.6-C1]。

**这份文件不含推荐。** 每项的最后一节只列三条路径各自的后果 ——
做 / 撤回 / 继续挂着。选哪条是委托方的判断,不是我的(§13)。

最后一项被拍板之后,这个文件应该消失。

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
