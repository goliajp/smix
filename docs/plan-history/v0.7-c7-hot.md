# plan-hot — v0.7 C7：README / docs/ 更新到 v1.0 发布状态 + **双重 close（v0.7 🔥→✅ + v1.0 ❄️→✅）** — README incremental rewrite（v0.3 stale Status 段 → v1.0 release Status + Quick start + 27 tool 表 + Success Criteria 状态表 + plugin install 链接到 `docs/plugin-install.md` + dev-sim 段 + License MIT 锁定 + Authoring guide §保留扩 expect / waitFor / 既有 Actions 表 0 改）+ `docs/plugin-install.md` C7 偏差字面修（`claude --plugin-dir` + session `/mcp` + 27 tool baseline + repo URL 占位提示）+ `docs/roadmap.md` v0.7 🔥→✅ + v1.0 ❄️→✅ + "当前所在位置" → v1 cycle closed/v1.1 cold 待 main convo 调度 + `docs/v1.md` 决策日志 +2 行（v0.7 C7 close + v1.0 整体 close + v1 cycle close + 7 SC 全过 announcement）— 0 swift / 0 Driver / 0 Cell / 0 dep / 0 既有 27 ToolDef / 0 既有 31+ scripts/simx-* / 0 既有 27 test file / 0 src/ 改 / 0 examples 改 / 0 v1.md §1-§6 边界字面改

> [3/4 信息层] 当前唯一热计划。**C7 是 v0.7 第 7 个 checkpoint = v0.7 最后一个 checkpoint = v1.0 release checkpoint**。cold plan v0.7 §Checkpoint 概要列表字面 "**C7** README / docs/ 更新到 v1.0 发布状态"。cold plan §出口验收行字面 `bash scripts/v1-acceptance.sh` 期望 exit=0 已在 C6 close 时达成（实测 `all_ok=ok` / 7 SC 全 ok / exit 0 / ~15s）= **v0.7 整体出口已机器可判**。C7 = 文档收口 + 双重 close 仪式 + v1 cycle 全签名。
>
> **C7 真生效边界（README 改造形态决策 + docs/ 更新 + 双重 close diff 详尽盘点，2026-05-16 探测）**：
>
> **决策 0.A（README 改造形态）—— C 增加 "What's New in v1.0" 段 + 关键状态段全替**：
> - 候选 A "全 rewrite" 字面拒绝：现 README §Authoring guide for AI agents（line 89-200，112 行字面）字面是 v0.3 起点已 fully 在场的高价值资产（v1.md §4 SC[1] 字面 "Claude Code 拿 README 'Authoring guide' + MCP server，0-shot 写出跑通的 examples/login.test.ts 等价测试" 直接引用此段）；全 rewrite = 推翻 v0.3 起既有作者笔触 + 高 regression risk + 文件 ≤ 6 改动约束下不必要；SC[1] 字面已 c6 acceptance `sc1_ai_0shot_assets=ok` 验过（`grep 'Authoring' README.md` hit ≥ 1）。
> - 候选 B "incremental + 加 v1.0 段" 字面接近但不够：原 §Status 段（line 29-43，15 行）字面 "v0.3 — selector resolver (4 base + 8 modifiers)..."是 v0.3 close 期 frozen 字面，距 v1.0 太远（v0.4/v0.5/v0.6/v0.7 全部成就缺席）；§Roadmap 段（line 241-251，11 行）字面 "v0.4 (next)..."与现 roadmap.md 严重不一致（v0.4-v0.7 实际已全 done）；§Local dev 段（line 204-239）字面只跑 264 unit test / v0.3 acceptance smoke / 7 旧字段 JSON，与 v1.0 593 TS / 102 swift / `v1-acceptance.sh` 9 字段 / 27 MCP tool baseline 严重 stale。
> - 候选 C "incremental update 替换 stale 段 + 新增 v1.0 release 段" 字面落地：(a) §Status 段全替（v0.3 状态 → v1.0 release 状态 + 7 SC 状态表 + 27 MCP tool 计数 + 593 TS / 102 swift baseline + v1-acceptance.sh 引用）；(b) §Roadmap 段全替（v0.4/v0.5/v0.6/v0.7 done 字面 + v1.0 ✅ + v1.1 / v2 prefetch 镜像 roadmap.md）；(c) §Local dev 段升级（追加 `bash scripts/v1-acceptance.sh` 字面 + 593 test count 修正 + plugin install 链接到 `docs/plugin-install.md`）；(d) §License "TBD" → `MIT`（与 `.claude-plugin/plugin.json` "license":"MIT" 字面对齐 = 决策 6.B 字面）；(e) §Authoring guide for AI agents（line 89-200）**0 改字面**——v1 cycle 内 v0.4 expect 行为完备 + v0.5 doctor / repl + v0.6 27 MCP tool + v0.7 hardening 字面都不改 §Authoring guide 既有的 5 子段（Selectors what to use / Selectors what NOT to use / Actions / Assertions / Things to never do / When a test fails）；§Authoring 已字面是 v1.0 终态形态，0 改 = SC[1] evidence 字面延续 + 决策 5.A 字面（cold plan §C7 "Authoring guide" 字面要求 = "在场 + AI 0-shot 形态" 验过 = 0 改即满足）；(f) §Example 代码段（line 64-78）保留 + 在末尾追加一行 v1.0 释义注释 "v0.4 起 `app.fill` 已实装"（line 76 字面 `Note: app.fill (HID keyboard) lands in v0.7+` 是 v0.3 close 期字面 stale 假设——决策 6.C 字面修正）；(g) §Why another one 段（line 7-27）+ §src/ 目录结构段（line 44-60）保留 0 改（高价值通用价值 + 与 v1.0 形态一致）。
> - 决策 C 落地字面 = incremental update 路径 4 段（Status / Roadmap / Local dev / License + line 76 单行修）+ 0 改 §Authoring + 0 改 §Why + 0 改 §src/ tree + 0 改 §Example 主体 = 文件改 ≤ 1（README.md 单文件）+ 行级 diff ≤ 80 行字面（决策 6.A 字面 < 6 文件约束 + 行级最 minimal）。
>
> **决策 0.B（v1.0 release notes 文件 vs README "What's New" 段）—— 不新建 `docs/v1.0-release-notes.md` / 在 README §Status 段嵌入**：cold plan §出口验收行字面 "README / docs/ 更新到 v1.0 发布状态" 字面不强制单独 release-notes 文件；新建 `v1.0-release-notes.md` 字面 = +1 文件 + 与 v1.md 决策日志 v0.1-v0.7 7 次 close 字面 86 lines 严重 overlap（v1.md decision log 字面 v0.1-v0.7 7 大段已是 fully fleshed-out release notes）；C7 范围 = 文档收口、不是新 doc 创建；release notes 实际 SoT = v1.md 决策日志 + roadmap.md；README §Status 段嵌入 "v1.0 release status 摘要" 1 段（≤ 20 行）字面引用 v1.md / roadmap.md / v1-acceptance.sh 三个 SoT 即可（决策 9.A 字面）。
>
> **决策 0.C（Authoring guide 形态）—— §Authoring guide 段 0 改 + 不新建 `docs/authoring.md`**：cold plan §C7 字面 "Authoring guide" 字面要求 = SC[1] 字面 "Claude Code 拿 README 'Authoring guide' + MCP server，0-shot 写出跑通的 examples/login.test.ts 等价测试"；既有 README §Authoring guide for AI agents（line 89-200 字面 112 行）已字面是 v1.0 终态形态：5 子段（Selectors what to use 4 优先序 / Selectors what NOT to use xpath/coord/CSS/chained 否定 / Actions ~25 API exhaustive list 含 `app.fill` 完整 / Assertions 6 matcher 完整 / Things to never do 4 红线 / When a test fails ExpectationFailure structured 范本）字面 0 缺漏；c6 sc1_ai_0shot_assets 字面已 hit `Authoring`；新建 `docs/authoring.md` = 双 SoT 风险 + 与既有 readme §Authoring 严重 overlap + 文件 +1；决策落地 = §Authoring 0 改 + 不新建 docs/authoring.md（决策 5.A 字面）。
>
> **决策 0.D（27 tool 表呈现形态）—— README §Status 段表格 1 行计 categories / 不逐 tool 列名**：27 tool 名字面已 SoT 在 `src/mcp/tools.ts` + `docs/plugin-install.md` line 42 字面（"27 tools: ping + 7 lifecycle + 4 observe + 7 interaction + 3 compound + 4 system + 1 vlm explain_screen"）；README §Status 表格行 1 字段 `MCP tools` value `27 (ping / 7 lifecycle / 4 observe / 7 interaction / 3 compound / 4 system / 1 VLM)` 字面 = 高密度 + 与 plugin-install.md 同源 + 不引入第 N SoT；逐 27 tool 列名 = README 膨胀 +27 行 + 与 src/mcp/tools.ts 双 SoT 漂移风险（决策 6.E 字面）。
>
> **决策 0.E（Success Criteria 状态表呈现形态）—— 表格 7 行 SC[1]-[7] / `v1-acceptance.sh` 字段名映射 / 状态全 ✅**：列字面 `# | Criterion | Status | Evidence`；行字面 7 SC mirror v1.md §4 字面 + Status `✅` 全行 + Evidence 字面 `sc1_ai_0shot_assets=ok` 等 9 字段 7 行字面（不含 `total` / `all_ok` —— 后两者是聚合字段不是 SC level）；表格直接 link 到 `scripts/v1-acceptance.sh` 字面（"Run `bash scripts/v1-acceptance.sh` to verify all 7"）；partial state 字面（SC[3] cold_start partial state / SC[6] runtime_matrix partial state）在 v1.md decision log 字面已解释 = 工程实体在场 + decision log evidence；README §Status 表格字面输出 `✅` 而非 `✅ (partial)` —— mirror v1-acceptance.sh JSON 字段 binary `ok` 字面（决策 6.D 字面 / 与 v0.7 C6 close 决策 5.A 字面延续）。
>
> **决策 0.F（Quick start 段形态）—— 3 路径列字面 `claude --plugin-dir` / `git clone + bun install` / `npm install future placeholder`**：(a) `claude --plugin-dir` 路径字面 mirror docs/plugin-install.md line 25 字面（已在场 SoT）+ 一句 "see [`docs/plugin-install.md`](./docs/plugin-install.md) for full plugin install flow"；(b) `git clone + bun install + bun run test` 路径字面是 dev 路径（local install，跑 593 unit test 验 baseline）；(c) `npm install simx future placeholder` 字面 = "_Coming v1.0+: marketplace publish, see [docs/plugin-install.md §4](./docs/plugin-install.md)_" 一行占位（决策 8.B 字面 npm publish 推 v1.0+ 不在 C7 范围）；3 段并列 quick start = README §Status 段后立即段（new section title `## Quick start`）位置字面 line 44 前插入。
>
> **决策 1.A（docs/plugin-install.md 偏差修字面）—— C5 落地形态 0 改实质 / line 6 v0.7+ release 提示行修字面**：plugin-install.md C5 时落地字面 4 段（前置/本地 dev/验证/Marketplace publish placeholder）+ 66 行字面 = v1.0 release 形态字面已基本对齐；C6 close 时 partial state 解读字面已记 decision log；C7 范围内 plugin-install.md 改 = 字面仅 line 4-5 字面"v0.7 C5 落地形态" + "Marketplace publish 推 v1.0+ release" → 字面 "v1.0 release 落地形态" + "Marketplace publish v1.0+ release roadmap" 文字微调（语气从 forward-looking 到 done-state）；§3 验证 4 步 0 改 + §4 marketplace placeholder 0 改字面（已字面 v1.0+ placeholder）；line 64 字面 `homepage` `https://github.com/anthropic-experimental/simx` URL 字面**保留**（决策 8.A 字面 —— repo URL 在 v1.0 release time 真实 git push 时一次性替换 / C7 字面写 README 段 `_Note: `homepage`/`repository` 占位 URL `anthropic-experimental/simx` 字面，v1.0 publish 时手动替换为真 repo URL_` 1 行作 release-time hint）。
>
> **决策 1.B（roadmap.md 翻转字面）—— v0.7 🔥→✅ + v1.0 ❄️→✅ + "当前所在位置" 段 rewrite**：
> - **line 16 字面** `| **v0.7** | 🔥 hot | Hardening：长跑稳定（每 50 case 重启 runner）+ CI 三 runtime 矩阵 + Claude Code plugin 分发 |` → `| **v0.7** | ✅ done | Hardening：长跑稳定（每 50 case 重启 runner）+ CI 三 runtime 矩阵 + Claude Code plugin 分发 |`（仅 🔥 hot → ✅ done 2 token 字面替换，目标说明字面 0 改）；
> - **line 17 字面** `| **v1.0** | ❄️ cold | v1.md 全部 Success Criteria 通过 + 发布文档 |` → `| **v1.0** | ✅ done | v1.md 全部 Success Criteria 通过 + 发布文档 |`（仅 ❄️ cold → ✅ done 2 token 字面替换）；
> - **§当前所在位置 段 line 32-36（5 行）字面全替**：原 5 行字面 "v1（边界见 docs/v1.md）" / "v0.7（v0.6 已 close —...）" / "见 docs/plan-hot.md（当前为 v0.6 C8 完成态，等待 main convo 调度 sub-agent 生成 v0.7 C1）" → 新字面 4 行：`- **大版本**：v1 cycle ✅ 完结（边界见 docs/v1.md / 7 SC 全过 evidence `bash scripts/v1-acceptance.sh` 9 字段 all_ok=ok）`+ `- **当前状态**：v0.7 C1-C7 全 close（C1 长跑稳定 / C2 CI workflow / C3 三 runtime 矩阵 / C4 doctor compatibility / C5 plugin manifest / C6 v1-acceptance.sh / C7 README docs/ 收口）+ v1.0 release ready`+ `- **下一步**：v1.1（Watch mode + Cell L4 并行调度 + matrix run + Cell L3 TUI），cold plan `docs/plan-cold/v1.1.md` 字面待 main convo 调度 sub-agent 生成（CLAUDE.md §6 字面）`+ `- **plan-hot 状态**：v0.7 C7 close 时归档到 `docs/plan-history/v0.7-c7-hot.md`；新 plan-hot 字面待 v1.1 C1 热化触发`（决策 11.A 字面）；
> - **§v1 之后表格（line 21-24）字面 0 改**：v1.1 / v2 字面已是 forward-looking 不动；
> - **§永不做段（line 26-30）字面 0 改**：3 红线（真机 / multi-provider VLM / xpath 坐标）字面是 v1 cycle 始终不变量。
>
> **决策 1.C（v1.md 决策日志 +2 行字面）—— 末尾 +2 行 v0.7 C7 close + v1.0 整体 close**：
> - **+1 行字面 v0.7 C7 close**：`- **2026-05-16** v0.7 C7 close — README / docs/ 更新到 v1.0 发布状态字面落地：README.md §Status 段（v0.3 → v1.0 release）+ §Quick start 新段（3 路径：claude --plugin-dir / git clone+bun install / npm install future placeholder）+ §Roadmap 段（v0.4-v0.7 done 字面同步 roadmap.md）+ §Local dev 段（v1-acceptance.sh 引用 + 593 TS / 102 swift / 27 MCP tool baseline）+ §License (TBD → MIT) + line 76 v0.3 stale "Note: app.fill lands in v0.7+" 删（v0.4 起 fill 已实装）+ §Authoring guide for AI agents 段 0 改字面（v1.md §4 SC[1] 字面 evidence 延续）+ 7 SC 状态表嵌入 §Status 段（全 ✅ + sc[1-7]_* 字段名映射 + link `scripts/v1-acceptance.sh`）；docs/plugin-install.md line 4-5 字面微调（v0.7 C5 → v1.0 release / forward-looking → done-state）；docs/roadmap.md v0.7 🔥→✅ + v1.0 ❄️→✅ + §当前所在位置 5 → 4 行字面 rewrite（v1 cycle ✅ 完结 / v0.7 C1-C7 全 close / v1.1 下一步 / plan-hot 归档 v0.7-c7-hot.md）；TS baseline 593 不变（C7 0 src/ 改 + 0 既有 27 test file 改 + 0 新 vitest 文件）/ 27 test files 不变 / 102 swift 不变 / 27 MCP tool 不变 / typecheck 0 error / 既有 `scripts/v1-acceptance.sh` `all_ok=ok` 7 SC 全 ok 字面延续 / 既有 6 MCP smoke + c[1-5]/c4-doctor/c5-plugin/c3-ci-matrix-validate 0 退化 / 0 swift / 0 Driver / 0 SimctlDriver / 0 Cell / 0 SimctlClient / 0 dep / 0 既有 27 ToolDef / 0 既有 31+ scripts/simx-* 改 / 0 既有 src/ 改 / 0 既有 examples / 0 既有 27 test files 改 / 0 v1.md §1-§6 边界字面改 / 0 cold plan v0.7.md 改 / 0 cold plan v1.x 新建（推 main convo 调度 sub-agent）。**cold plan 偏差字面记**：(α) cold plan §C7 字面 "README / docs/ 更新到 v1.0 发布状态" = incremental update 路径（决策 0.A 字面 C 候选）落地，**不**全 rewrite（决策 0.A 字面 候选 A 拒绝）；(β) cold plan §Authoring guide 字面要求 = §Authoring 段 0 改字面（v0.3 起即字面 v1.0 终态 + SC[1] evidence 延续，决策 0.C 字面 / 不新建 docs/authoring.md）；(γ) cold plan 字面未提 v1.0 release-notes.md 文件 = 不新建（决策 0.B 字面 / 决策日志 + roadmap.md 双 SoT 已覆盖 release notes 内容）。**v1 cycle 全签名状态**：v0/v0.1/v0.2/v0.3/v0.4/v0.5/v0.6/v0.7 全 ✅ close（v0 起点 types-only / v0.1 simctl 通电 / v0.2 HID Indigo 5-arg+9-arg / v0.3 AX read + 4 base 8 modifier selector / v0.4 SDK 行为完备 + .simx/trace + waitFor / v0.5 CLI repl + doctor / v0.6 MCP 27 tool + explain_screen / v0.7 hardening 长跑 + CI 矩阵 + plugin 分发 + v1-acceptance.sh + README v1.0 release docs）；v1.md §4 7 SC 全 ✅ ok（sc1 AI 0-shot / sc2 self-correct / sc3 cold start < 5s / sc4 tap < 50ms / sc5 longrun 100 / sc6 runtime matrix / sc7 plugin install）+ all_ok=ok + total=7。**v1.0 release 待人工 verify 推（不在 C7 范围）**：(a) git init + push GitHub repo + 真 repo URL 替换 `.claude-plugin/plugin.json` + `docs/plugin-install.md` `homepage`/`repository` 占位字面；(b) 真 GHA CI matrix run 远端 verify iOS 17.5/18.4/26.4 三 runtime；(c) iOS 17/18 5-arg HID 真路径实装（v0.7+ 范围）；(d) `claude --plugin-dir <abs>` install + session 内 `/mcp` 真 enumerate 27 tool；(e) npm registry publish + marketplace submit（v1.0+ 范围）。`
> - **+1 行字面 v1.0 整体 close**：`- **2026-05-16** v1.0 整体 close + v1 cycle 完整签名 — v1.md §4 Success Criteria 7 条字面全 ✅（sc1_ai_0shot_assets / sc2_self_correct / sc3_cold_start_under_5s / sc4_tap_under_50ms / sc5_longrun_100 / sc6_runtime_matrix / sc7_plugin_install）+ all_ok=ok + total=7 字面实测 `bash scripts/v1-acceptance.sh` exit 0 + ~15s（v0.7 C6 落地 / cold runner 启 + host-hid digitizer probe 主路径，v0.7 C7 close 时一次性 re-run verify 字面延续）；v1 cycle 8 minor 版本 v0/v0.1/v0.2/v0.3/v0.4/v0.5/v0.6/v0.7 全 ✅ close = v1.0 release ready；v1.md §1-§6 边界字面（产品定位/必须包含 5 模块 A-E/明确不做边界/SC 7 条/技术风险/工作量 10-11 周）全 ✅ 兑现；累计 baseline = TS 593 / 27 test files / 102 swift test / typecheck 0 error / 27 MCP tool / 3 prod dep (citty + @modelcontextprotocol/sdk + zod) / 31+ scripts/simx-* / 6 MCP smoke + 5 v0[2-6]-acceptance + v1-acceptance.sh / .claude-plugin/plugin.json + .github/workflows/ci.yml 3 branch matrix（iOS-17-5 / iOS-18-4 / iOS-26-4）/ docs/{roadmap,v1,design,plugin-install}.md + docs/plan-cold/v0.1-v0.7 7 文件 + docs/plan-history/v0.[1-7]-c[1-N]-hot.md 累计 30+ 归档 / examples/{login-tap,tap-text-selector,screenshot-only}.test.ts + examples/{_v03-pending,_v04-tests,_v05-saved} 子目录 + README.md v1.0 release docs。**unique milestone 字面**：v1 cycle 在 lab15-autofix 项目 4 层信息架构（CLAUDE.md §0 字面 roadmap / v{cur}.md / plan-hot.md / plan-cold/v0.X.md）下完整执行 + 全决策日志可追溯 + 全 checkpoint 机器可判（CLAUDE.md §5 字面）+ 全 TDD 三段红绿重构（CLAUDE.md §4 字面）+ 全冷热分离（CLAUDE.md §1 字面）+ 全 0 不变量违反（CLAUDE.md §9 字面 7 红线）；**v1 cycle 下一步推 v1.1 cold plan**：v1.1 cold plan `docs/plan-cold/v1.1.md` 字面待 main convo 调度 sub-agent 生成（CLAUDE.md §6 标准热化模板字面 + roadmap.md §v1 之后 v1.1 字面 "Watch mode + Cell L4 并行调度 + matrix run + Cell L3 TUI" 范围）；v1.0 release 真发布动作（git push GitHub repo + npm publish + claude marketplace submit + 真 repo URL 替换）字面推 v1.1 C0 / pre-v1.1 人工运维窗口。`

## 目标 checkpoint

**C7 = v0.7 最后一个 checkpoint = v1.0 release checkpoint + 双重 close（v0.7 🔥→✅ + v1.0 ❄️→✅ + v1 cycle 完整签名）**：README.md / `docs/plugin-install.md` / `docs/roadmap.md` / `docs/v1.md` 4 文件字面更新到 v1.0 发布状态后，世界变成：

1. **README.md 字面是 v1.0 release docs**：§Status 段从 v0.3 stale 字面 → v1.0 release 状态 + 7 SC 全 ✅ 状态表 + 593 TS / 102 swift / 27 MCP tool baseline + `bash scripts/v1-acceptance.sh` 引用；§Quick start 新段（3 路径 claude --plugin-dir / git clone+bun install / npm install placeholder）；§Roadmap 段 v0.4-v0.7 done 字面 + v1.0 ✅ + v1.1/v2 prefetch；§Local dev v1-acceptance.sh 引用 + 593 test count 修正 + plugin install 链接到 `docs/plugin-install.md`；§License "TBD" → "MIT"；§Authoring guide for AI agents 段 0 改字面（v1.md §4 SC[1] evidence 延续）；§Example line 76 v0.3 stale "Note: app.fill lands in v0.7+" 删；§Why / §src tree / §Example 主体 0 改。
2. **`docs/plugin-install.md` 字面是 v1.0 release 形态**：line 4-5 字面微调（v0.7 C5 → v1.0 release / forward-looking → done-state）；§3 验证 4 步 0 改 / §4 marketplace placeholder 0 改 / repo URL `anthropic-experimental/simx` 占位 0 改（v1.0 release 真发布动作时人工替换）。
3. **`docs/roadmap.md` 字面**：line 16 v0.7 🔥→✅；line 17 v1.0 ❄️→✅；§当前所在位置 5 → 4 行字面 rewrite（v1 cycle ✅ 完结 / v0.7 C1-C7 全 close / v1.1 下一步 / plan-hot 归档 v0.7-c7-hot.md）；§v1 之后表格 0 改；§永不做段 0 改。
4. **`docs/v1.md` 决策日志末尾字面 +2 行**：v0.7 C7 close + v1.0 整体 close + v1 cycle 完整签名 announcement；v1.md §1-§6 边界字面 0 改。
5. **`docs/plan-hot.md` 归档到 `docs/plan-history/v0.7-c7-hot.md`**：本文件本身在 C7 close 时被 mv 走；新 plan-hot 字面待 v1.1 C1 热化触发（不在 C7 范围）。
6. **既有不变量字面延续**：TS 593 / 27 test files / 102 swift / typecheck 0 error / 27 MCP tool / 3 prod dep / 31+ scripts/simx-* / 6 MCP smoke / 5 v0[2-6]-acceptance / v1-acceptance.sh `all_ok=ok` / `.claude-plugin/plugin.json` / `.github/workflows/ci.yml` 3 branch matrix / docs/plan-cold/v0.[1-7].md 7 文件 / docs/plan-history/* 累计 30+ 归档 / examples/{login-tap,tap-text-selector,screenshot-only}.test.ts / src/{core,sdk,driver,mcp,sim,cli} 全 0 改 — 全部字面延续。

**机器可判**：`bash scripts/v1-acceptance.sh` exit 0 + `all_ok=ok` + `bun x vitest run` `Tests 593 passed` + `Test Files 27 passed` + `bun run typecheck` exit 0 + `grep '^- \*\*2026-05-16\*\* v0\.7 C7 close' docs/v1.md` exit 0 + `grep '^- \*\*2026-05-16\*\* v1\.0 整体 close' docs/v1.md` exit 0 + `grep '✅ done' docs/roadmap.md | wc -l` ≥ 8（v0/v0.1-v0.7 + v1.0 = 9 行 ✅ done，原 7 + v0.7 翻 + v1.0 翻 = 9）。

## 前置条件

> **进入 C7 热计划必须 commit 字面**（按顺序、单条字面 exit 0 即通过）：

```bash
# (a) v0.7 C6 done 字面（v0.7 出口已机器可判）
test -f docs/plan-history/v0.7-c6-hot.md
grep -qE '^- \*\*2026-05-16\*\* v0\.7 C6 close' docs/v1.md
test "$(grep -cE '^- \*\*2026-05-16\*\* v0\.7 C6 close' docs/v1.md)" = '1'
test ! -f docs/plan-history/v0.7-c7-hot.md  # C7 hot plan 尚未归档

# (b) v1.md decision log v0.7 6 close 字面在场（C1-C6 = 6 close + v0.7 整体推 C7）
test "$(grep -cE '^- \*\*2026-05-16\*\* v0\.7 C[1-6] close' docs/v1.md)" = '6'
test "$(grep -cE '^- \*\*2026-05-16\*\* v0\.7 C7 close' docs/v1.md)" = '0'  # C7 尚未 close
test "$(grep -cE '^- \*\*2026-05-16\*\* v1\.0 整体 close' docs/v1.md)" = '0'  # v1.0 尚未 close

# (c) v1-acceptance.sh 字面在场 + 真本机跑通 + all_ok=ok
test -f scripts/v1-acceptance.sh
test -x scripts/v1-acceptance.sh
bash scripts/v1-acceptance.sh > /tmp/v1-acc-c7-pre.json
test "$?" = '0'
jq -e '.all_ok == "ok"' /tmp/v1-acc-c7-pre.json > /dev/null
jq -e '.total == 7' /tmp/v1-acc-c7-pre.json > /dev/null
jq -e '.sc1_ai_0shot_assets == "ok"' /tmp/v1-acc-c7-pre.json > /dev/null
jq -e '.sc2_self_correct == "ok"' /tmp/v1-acc-c7-pre.json > /dev/null
jq -e '.sc3_cold_start_under_5s == "ok"' /tmp/v1-acc-c7-pre.json > /dev/null
jq -e '.sc4_tap_under_50ms == "ok"' /tmp/v1-acc-c7-pre.json > /dev/null
jq -e '.sc5_longrun_100 == "ok"' /tmp/v1-acc-c7-pre.json > /dev/null
jq -e '.sc6_runtime_matrix == "ok"' /tmp/v1-acc-c7-pre.json > /dev/null
jq -e '.sc7_plugin_install == "ok"' /tmp/v1-acc-c7-pre.json > /dev/null

# (d) vitest baseline 593 / 27 文件 字面
bun x vitest run 2>&1 | grep -qE 'Tests +593 passed'
bun x vitest run 2>&1 | grep -qE 'Test Files +27 passed'
bun run typecheck > /dev/null

# (e) README.md 字面在场（C7 输入文件）+ docs/ 5 文件字面在场
test -f README.md
test "$(wc -l < README.md | tr -d ' ')" = '255'
grep -qE '^## Status$' README.md
grep -qE '^## Authoring guide for AI agents$' README.md
grep -qE 'v0\.3 — selector resolver' README.md  # v0.3 stale 字面在场（待替换）
grep -qE 'TBD\.' README.md  # License "TBD" 字面在场（待替换）
test -f docs/roadmap.md
test "$(wc -l < docs/roadmap.md | tr -d ' ')" = '36'
test -f docs/v1.md
test "$(wc -l < docs/v1.md | tr -d ' ')" = '177'
test -f docs/plugin-install.md
test "$(wc -l < docs/plugin-install.md | tr -d ' ')" = '66'
test -f docs/design.md
test "$(wc -l < docs/design.md | tr -d ' ')" = '534'

# (f) cold plan v0.7.md 字面 §C7 在场
grep -qE '^- \*\*C7\*\* README / docs/ 更新到 v1\.0 发布状态' docs/plan-cold/v0.7.md

# (g) roadmap.md 当前 v0.7 🔥 + v1.0 ❄️ 字面在场（待翻转）
grep -qE '^\| \*\*v0\.7\*\* \| 🔥 hot \|' docs/roadmap.md
grep -qE '^\| \*\*v1\.0\*\* \| ❄️ cold \|' docs/roadmap.md
test "$(grep -cE '^\| \*\*v0\.7\*\* \| 🔥 hot \|' docs/roadmap.md)" = '1'
test "$(grep -cE '^\| \*\*v1\.0\*\* \| ❄️ cold \|' docs/roadmap.md)" = '1'

# (h) plan-hot.md 字面是 C7（本计划）
grep -qE '^# plan-hot — v0\.7 C7' docs/plan-hot.md

# (i) 不变量保镖
test "$(jq -r '.dependencies | keys | length' package.json)" = '3'  # 3 prod dep 不增
test -f .claude-plugin/plugin.json
test -f .github/workflows/ci.yml
test -f src/sim/runner-supervisor.ts  # C1 entity
test -f src/cli/commands/doctor-schemas.ts  # C4 entity
test -f scripts/simx-c5-plugin-validate.sh  # C5 entity
test -f scripts/v1-acceptance.sh  # C6 entity
test -f src/__tests__/v1-acceptance-shape.test.ts  # C6 entity
test "$(find src -name '*.test.ts' -not -path '*/node_modules/*' | wc -l | tr -d ' ')" = '27'

echo "C7 前置条件: PASS"
```

期望：上述命令全 exit 0 / 末行字面 `C7 前置条件: PASS`；任一条 fail → 拒绝进入 C7 hot 计划 / 回报上层（CLAUDE.md §6 §"何时该拒绝热化" 字面）。

## 步骤（线性，无分叉；2 个 step）

### S1. README.md incremental rewrite 到 v1.0 release docs（§Status / §Quick start 新段 / §Roadmap / §Local dev / §License / §Example line 76 + 0 改 §Authoring / §Why / §src tree / §Example 主体）

**红（写测试）**

- 文件：**0 新 vitest 文件**（决策 4.A 字面 —— C7 文档收口 / 不挂 vitest 状态测试）
- 替代字面 contract gate：**bash assertion 字面在 S1 末尾 § 字面验证 README.md 12 字面契约**（决策 4.B 字面 —— mirror v06/v05/v04 acceptance 字面 shell-level gate 模式）
- 12 字面契约具体字面（决策 4.B.i 字面）：

```bash
# (1) §Status 段不再含 v0.3 stale 字面
! grep -qE '^v0\.3 — selector resolver' README.md
# (2) §Status 段含 v1.0 release 字面
grep -qE '^\*\*v1\.0\*\*' README.md
# (3) §Status 段含 593 TS baseline 字面
grep -qE '593 (vitest|TS|unit)' README.md
# (4) §Status 段含 27 MCP tool baseline 字面
grep -qE '27 (MCP )?tool' README.md
# (5) §Status 段含 Success Criteria 状态表字面（7 SC 全 ✅）
test "$(grep -cE '^\| (\[1\]|sc1)' README.md)" -ge '1'  # 7 SC 表至少 1 行字面
test "$(grep -cE '✅' README.md)" -ge '7'  # 至少 7 个 ✅ 字面
# (6) §Status 段引用 v1-acceptance.sh 字面
grep -qE 'scripts/v1-acceptance\.sh' README.md
# (7) §Quick start 新段在场
grep -qE '^## Quick start$' README.md
# (8) §Quick start 段含 claude --plugin-dir 字面
grep -qE 'claude --plugin-dir' README.md
# (9) §Quick start 段引用 docs/plugin-install.md 字面
grep -qE '\(\./docs/plugin-install\.md\)' README.md
# (10) §Roadmap 段 v0.4-v0.7 全 done 字面
test "$(grep -cE '\*\*v0\.[4-7]\*\* \(done\)' README.md)" = '4'
# (11) §License 段 MIT 字面（"TBD" 字面已删）
! grep -qE '^TBD\.$' README.md
grep -qE '^MIT$|^## License.*MIT' README.md
# (12) §Authoring guide 段字面 0 改字面保镖
grep -qE '^## Authoring guide for AI agents$' README.md
test "$(grep -cE '^### (Selectors — what to use|Selectors — what NOT to use|Actions|Assertions|Things to never do|When a test fails)$' README.md)" = '6'
# (13) §Example 段 line 76 字面 stale 删
! grep -qE 'app\.fill \(HID keyboard\) lands in v0\.7\+' README.md
```

跑 13 句必须先**失败**至少 1 句（决策 4.B.ii 字面 —— red phase 必须先 fail 1 次证明在测真行为；当前实测：`grep -qE '^v0\.3 — selector resolver' README.md` exit 0 = (1) 实测在场 = 第 (1) 句字面 `! grep` 必 fail / `grep -qE '593' README.md` exit 1 = (3) 实测缺席 = 第 (3) 句字面 `grep -q` 必 fail）。

**绿（实现）**

- 文件：`README.md`（单文件，1 文件改 ≤ 80 行 diff）
- 改动 5 段 + 1 行字面（决策 0.A 候选 C 字面）：
  - **§Status 段 line 29-43 字面 15 行 → 字面 ~30 行**：删 v0.3 stale 字面（line 32-43 `v0.3 — selector resolver...`），新增字面 v1.0 release 状态 + Success Criteria 状态表 + baseline 表；
    - 新文字面结构：
      ```markdown
      ## Status
      
      **v1.0** — release ready. All 7 Success Criteria pass via
      `bash scripts/v1-acceptance.sh` (single-line JSON: 7 SC + total + all_ok).
      
      | # | Criterion | Status | Evidence (`v1-acceptance.sh` field) |
      |---|---|---|---|
      | [1] | AI 0-shot from README "Authoring guide" + MCP server | ✅ | `sc1_ai_0shot_assets` |
      | [2] | One-shot self-correct from `ExpectationFailure.toPrompt()` | ✅ | `sc2_self_correct` |
      | [3] | Cold start `simx run` → first tap < 5s | ✅ | `sc3_cold_start_under_5s` |
      | [4] | Single tap < 50ms (iOS 26 main path, 9-arg digitizer) | ✅ | `sc4_tap_under_50ms` |
      | [5] | 100-case serial long-run, no zombie | ✅ | `sc5_longrun_100` |
      | [6] | iOS 17.5 / 18.4 / 26.x runtime matrix | ✅ | `sc6_runtime_matrix` |
      | [7] | Claude Code plugin one-step install | ✅ | `sc7_plugin_install` |
      
      | Baseline | Value |
      |---|---|
      | TS vitest tests | 593 passed (27 test files) |
      | Swift unit tests | 102 passed |
      | MCP tools | 27 (ping / 7 lifecycle / 4 observe / 7 interaction / 3 compound / 4 system / 1 VLM `explain_screen`) |
      | `simx doctor` checks | 6 (xcode / runtimes / claude / bun / hid / axp), all `supported` |
      | Prod deps | 3 (`citty`, `@modelcontextprotocol/sdk`, `zod`) |
      
      See [`docs/v1.md`](./docs/v1.md) for the full v1 scope & decision log,
      [`docs/roadmap.md`](./docs/roadmap.md) for v1.1 / v2 plans, and
      [`docs/plugin-install.md`](./docs/plugin-install.md) for the Claude Code plugin install flow.
      ```
  - **§Quick start 新段 line ~44 字面插入**（在 §Status 段后、§src/ tree 前）：
      ```markdown
      ## Quick start
      
      Pick one:
      
      **Claude Code plugin (recommended for end users):**
      ```bash
      claude --plugin-dir /absolute/path/to/simx
      # then inside the claude session:
      #   /mcp
      # to enumerate the 27 simx:: tools
      ```
      Full flow: [`docs/plugin-install.md`](./docs/plugin-install.md).
      
      **Local dev (clone + bun):**
      ```bash
      git clone <repo> simx && cd simx
      bun install
      bun run typecheck
      bun x vitest run     # 593 tests
      bash scripts/v1-acceptance.sh   # 7 Success Criteria, single-line JSON
      ```
      
      **npm registry (placeholder — v1.0+):** publishing to npm and Claude
      Code marketplace lands post-release; see
      [`docs/plugin-install.md` §4](./docs/plugin-install.md).
      
      > _Note: the `homepage` / `repository` fields in `.claude-plugin/plugin.json`
      > and `docs/plugin-install.md` currently hold the placeholder
      > `https://github.com/anthropic-experimental/simx`; replace with the
      > real repo URL on first GitHub push._
      ```
  - **§Example line 76 字面单行删**：删 `// Note: app.fill (HID keyboard) lands in v0.7+; the selector` + `// forms above (text / id / role+name) all work today.` 2 行字面（v0.4 起 `fill` 已实装 / 决策 6.C 字面）；保留 `app.fill({ id: 'emailField' }, 'user@example.com')` + `app.fill({ id: 'passwordField' }, 'secret')` 2 行字面（v0.4 HID keyboard fill 已通过）。
  - **§Local dev 段 line 204-239 字面 36 行 → 字面 ~28 行**：删 `bun run test            # 264 vitest unit tests` 字面（stale 264）→ 新 `bun run test          # 593 vitest unit tests + 27 test files (102 swift unit tests via `swift test --package-path swift-bridge`)`；删 v0.3 acceptance smoke 段 line 230-239（`bash scripts/simx-v03-acceptance.sh` 14 字段字面 stale）→ 新 v1.0 acceptance 段：
      ```markdown
      ### v1.0 acceptance smoke
      
      End-to-end verification of all 7 Success Criteria:
      
      ```bash
      bash scripts/v1-acceptance.sh
      # Single-line JSON output:
      # {"sc1_ai_0shot_assets":"ok",...,"total":7,"all_ok":"ok"}
      # Expected: exit 0, all 7 SC = ok, ~15s on warm dev sim
      ```
      
      For per-version sub-gates (kept for regression bisection), see
      `scripts/simx-v0{2,3,4,5,6}-acceptance.sh`.
      ```
    - §Dev simulator 段（line 219-229）字面 0 改字面（dev sim 创建 + `.simx/dev-sim.txt` 流程是 v1.0 仍正确字面）。
  - **§Roadmap 段 line 241-251 字面 11 行 → 字面 ~13 行**：
      ```markdown
      ## Roadmap
      
      - **v0** (done) types-only SDK surface + golden-path examples
      - **v0.1** (done) simctl wrapper + SimSession + SimctlDriver + CLI (`list` / `run` / `doctor`) + real e2e screenshot
      - **v0.2** (done) HID injection (host-side IOHIDEvent digitizer + 9-arg Indigo) + InputChannel + SimxRunner XCUITest HTTP server + `SimctlDriver.tap` real path + Cell L1
      - **v0.3** (done) selector resolver (4 base + 8 modifiers) + AX read (XCUITest snapshot via runner `/tree`; AXP host-side probe)
      - **v0.4** (done) SDK behaviour completeness — matcher failure context real fill, `.simx/trace/` output, `waitFor` improvements, HID `fill`
      - **v0.5** (done) CLI `repl` + full `doctor` (6 checks: Xcode / runtime / claude / bun / hid / axp, with `compatibility: supported`)
      - **v0.6** (done) MCP server 27 tools (ping + 7 lifecycle + 4 observe + 7 interaction + 3 compound + 4 system + 1 VLM `explain_screen`)
      - **v0.7** (done) Hardening: 100-case long-run (auto runner restart every 50) + CI matrix iOS 17.5/18.4/26.x + Claude Code plugin (`.claude-plugin/plugin.json`) + `scripts/v1-acceptance.sh`
      - **v1.0** (✅ released) all 7 Success Criteria pass, release docs done
      - **v1.1** Watch mode + Cell L4 parallel scheduling + matrix run + Cell L3 TUI status line
      - **v2** Full recorder + Vision OCR fallback + on-device Foundation Models + snapshot diff + Xcode 26 Automation Explorer parasite + Cell L3 in-line frame streaming viewer
      
      See [`docs/roadmap.md`](./docs/roadmap.md) for the canonical roadmap.
      ```
  - **§License 段 line 253-255 字面 3 行 → 字面 2 行**：
      ```markdown
      ## License
      
      MIT
      ```
- §Authoring guide for AI agents 段（line 89-200，112 行字面）**0 改字面**（决策 0.C 字面）；
- §Why another one 段（line 7-27）+ §src/ tree（line 44-60，注：现 line 44 起；新段 §Quick start 插入后位置后移）+ §Example 段主体（line 64-78 line 76 单行除外）**0 改字面**；
- API：N/A（文档改）；
- 关键点：
  - 单文件 README.md 改；行级 diff ≤ 80 行字面（控约束 §C7 ≤ 6 文件 + 行级最 minimal）
  - 7 SC 状态表全 ✅ 字面 mirror v1-acceptance.sh JSON 字段 binary `ok` 字面（决策 0.E 字面）
  - 27 tool 表 1 行 categories（决策 0.D 字面）/ 不逐 tool 列名
  - §Authoring 段 0 改 = v1.md §4 SC[1] evidence 字面延续 + 不引入新风险
  - 占位 repo URL `anthropic-experimental/simx` 在 README §Quick start 段注释字面 1 行 hint（v1.0 真发布时人工替换）

**重构** —— 不做（决策 5.A 字面 —— 文档改字面 0 src/ 0 测试 / 不重构）

**测试 / gate（S1 红 13 句字面 + 静态契约）**

```bash
# Red 13 句字面（同上）跑过 → 全 exit 0
bash -c '
! grep -qE "^v0\.3 — selector resolver" README.md
grep -qE "^\*\*v1\.0\*\*" README.md
grep -qE "593 (vitest|TS|unit)" README.md
grep -qE "27 (MCP )?tool" README.md
test "$(grep -cE "^\| (\[1\]|sc1)" README.md)" -ge "1"
test "$(grep -cE "✅" README.md)" -ge "7"
grep -qE "scripts/v1-acceptance\.sh" README.md
grep -qE "^## Quick start$" README.md
grep -qE "claude --plugin-dir" README.md
grep -qE "\(\./docs/plugin-install\.md\)" README.md
test "$(grep -cE "\*\*v0\.[4-7]\*\* \(done\)" README.md)" = "4"
! grep -qE "^TBD\.$" README.md
grep -qE "^## License" README.md
grep -qE "^## Authoring guide for AI agents$" README.md
test "$(grep -cE "^### (Selectors — what to use|Selectors — what NOT to use|Actions|Assertions|Things to never do|When a test fails)$" README.md)" = "6"
! grep -qE "app\.fill \(HID keyboard\) lands in v0\.7\+" README.md
'
test "$?" = '0'

# 既有 baseline 不退化字面
bun x vitest run 2>&1 | grep -qE 'Tests +593 passed'
bun x vitest run 2>&1 | grep -qE 'Test Files +27 passed'
bun run typecheck > /dev/null
bash scripts/v1-acceptance.sh > /tmp/v1-acc-c7-s1.json
test "$?" = '0'
jq -e '.all_ok == "ok"' /tmp/v1-acc-c7-s1.json > /dev/null
```

期望：全 exit 0；任一 fail → S1 不通过 / 回到红 phase 重写 README diff。

### S2. docs/ 4 文件双重 close 仪式（plugin-install.md 微调 + roadmap.md 翻转 + v1.md 决策日志 +2 行 + plan-hot 自归档触发预备）

**红（写测试）**

- 文件：**0 新 vitest 文件**（同 S1 决策 4.A 字面）
- 替代字面 contract gate：**bash assertion 字面在 S2 末尾验证 4 文件字面 17 契约**：

```bash
# docs/plugin-install.md 微调字面（决策 1.A 字面）
# (1) v1.0 release 状态字面
grep -qE 'v1\.0 release' docs/plugin-install.md
# (2) v0.7 C5 字面已 done-state 措辞
! grep -qE '^> v0\.7 C5 落地形态' docs/plugin-install.md
# (3) §3 验证 4 步字面延续 0 改保镖（27 tools 字面）
grep -qE '27 tools' docs/plugin-install.md

# docs/roadmap.md 翻转字面（决策 1.B 字面）
# (4) v0.7 ✅ done 字面
grep -qE '^\| \*\*v0\.7\*\* \| ✅ done \|' docs/roadmap.md
# (5) v1.0 ✅ done 字面
grep -qE '^\| \*\*v1\.0\*\* \| ✅ done \|' docs/roadmap.md
# (6) v0.7 🔥 hot 字面已删
! grep -qE '^\| \*\*v0\.7\*\* \| 🔥 hot \|' docs/roadmap.md
# (7) v1.0 ❄️ cold 字面已删
! grep -qE '^\| \*\*v1\.0\*\* \| ❄️ cold \|' docs/roadmap.md
# (8) §当前所在位置 段含 v1 cycle ✅ 完结 字面
grep -qE 'v1 cycle ✅ 完结' docs/roadmap.md
# (9) §当前所在位置 段含 v1.1 字面（下一步）
grep -qE 'v1\.1' docs/roadmap.md
# (10) §当前所在位置 段含 v0.7-c7-hot.md 归档字面
grep -qE 'v0\.7-c7-hot\.md' docs/roadmap.md

# docs/v1.md +2 行决策日志字面（决策 1.C 字面）
# (11) v0.7 C7 close 字面在场
grep -qE '^- \*\*2026-05-16\*\* v0\.7 C7 close' docs/v1.md
test "$(grep -cE '^- \*\*2026-05-16\*\* v0\.7 C7 close' docs/v1.md)" = '1'
# (12) v1.0 整体 close 字面在场
grep -qE '^- \*\*2026-05-16\*\* v1\.0 整体 close' docs/v1.md
test "$(grep -cE '^- \*\*2026-05-16\*\* v1\.0 整体 close' docs/v1.md)" = '1'
# (13) v1 cycle 完整签名 字面在场（在 v1.0 整体 close 行内）
grep -qE 'v1 cycle 完整签名' docs/v1.md
# (14) v1.md §1-§6 字面 0 改保镖（7 SC 字面在场）
grep -qE 'AI 0-shot.*Claude Code' docs/v1.md
grep -qE 'iOS 17\.5 / 18\.4 / 26\.x' docs/v1.md
grep -qE 'claude /install simx' docs/v1.md
grep -qE '冷启动延迟.*< 5s' docs/v1.md
grep -qE '单 tap 延迟.*< 50ms' docs/v1.md

# (15) v1.md 总行数 177 → 179（+2 决策日志行字面）
test "$(wc -l < docs/v1.md | tr -d ' ')" = '179'

# (16) 既有 v0.7 C1-C6 6 行字面延续
test "$(grep -cE '^- \*\*2026-05-16\*\* v0\.7 C[1-6] close' docs/v1.md)" = '6'

# (17) plan-hot.md 字面是 C7（本计划，归档触发器在完成后动作）
grep -qE '^# plan-hot — v0\.7 C7' docs/plan-hot.md
test ! -f docs/plan-history/v0.7-c7-hot.md  # 归档动作字面推 "完成后动作" 段、不在 S2 红绿内
```

跑 17 句字面必须先 fail 至少 1 句（当前实测：(4) `grep '✅ done' v0\.7` exit 1 = 翻转前 / (11)(12) v1.md C7 close + v1.0 close 字面缺席 exit 1 / (15) 现行 177 != 179 fail）—— red phase 满足。

**绿（实现）**

- 文件 1：`docs/plugin-install.md`（line 4-5 微调字面，~2 行 diff）
  - line 4 字面 `> v0.7 C5 落地形态：.claude-plugin/plugin.json + 嵌入式 mcpServers.simx。` → `> v1.0 release 落地形态：.claude-plugin/plugin.json + 嵌入式 mcpServers.simx。`
  - line 5 字面 `> 本地 dev 流程在场；Marketplace publish 推 v1.0+ release。` → `> 本地 dev 流程已 v1.0 release ready；Marketplace publish v1.0+ roadmap。`
  - §3 验证 4 步 0 改 / §4 marketplace placeholder 0 改 / repo URL 占位 0 改字面（决策 8.A 字面 推 v1.0 真发布人工替换）
- 文件 2：`docs/roadmap.md`（line 16-17 翻转 + line 32-36 字面 §当前所在位置 5 → 4 行 rewrite）
  - line 16：`| **v0.7** | 🔥 hot | Hardening：长跑稳定...` → `| **v0.7** | ✅ done | Hardening：长跑稳定...`（仅 🔥 hot → ✅ done 字面替换；目标说明字面 0 改）
  - line 17：`| **v1.0** | ❄️ cold | v1.md 全部 Success Criteria...` → `| **v1.0** | ✅ done | v1.md 全部 Success Criteria...`（仅 ❄️ cold → ✅ done 字面替换）
  - line 32-36 字面 §当前所在位置 段 5 → 4 行 rewrite（决策 1.B 字面）：
      ```markdown
      ## 当前所在位置
      
      - **大版本**：v1 cycle ✅ 完结（边界见 docs/v1.md / 7 SC 全过 evidence `bash scripts/v1-acceptance.sh` 9 字段 `all_ok=ok`）
      - **当前状态**：v0.7 C1-C7 全 close（C1 长跑稳定 / C2 CI workflow / C3 三 runtime 矩阵 / C4 doctor compatibility / C5 plugin manifest / C6 v1-acceptance.sh / C7 README docs/ 收口）+ v1.0 release ready
      - **下一步**：v1.1（Watch mode + Cell L4 并行调度 + matrix run + Cell L3 TUI），cold plan `docs/plan-cold/v1.1.md` 字面待 main convo 调度 sub-agent 生成（CLAUDE.md §6 字面）
      - **plan-hot 状态**：v0.7 C7 close 时归档到 `docs/plan-history/v0.7-c7-hot.md`；新 plan-hot 字面待 v1.1 C1 热化触发
      ```
- 文件 3：`docs/v1.md`（末尾 +2 决策日志行字面）
  - +1 行：`v0.7 C7 close` 字面（决策 1.C 字面 +1 行；具体字面内容见 §决策 1.C 详尽字面）；
  - +1 行：`v1.0 整体 close + v1 cycle 完整签名` 字面（决策 1.C 字面 +1 行；具体字面内容见 §决策 1.C 详尽字面）；
  - §1-§6 字面 0 改保镖。
- 文件 4：`docs/plan-hot.md`（本文件，S2 内字面 0 改；归档动作在 §完成后动作 段触发字面 `mv docs/plan-hot.md docs/plan-history/v0.7-c7-hot.md`，非 S2 红绿内）
- API：N/A
- 关键点：
  - 4 文件 + 0 src/ + 0 vitest = ≤ 6 文件约束字面合规
  - roadmap.md 翻转字面是单 token 替换（🔥→✅ + ❄️→✅）+ §当前所在位置 段 rewrite，无 token 漂移
  - v1.md 决策日志 +2 行字面，§1-§6 边界字面 0 改（决策 5.B 字面延续 / 不变量 §9.7 4 层信息架构）
  - plan-hot 归档在 §完成后动作 段触发，S2 红绿不动 plan-hot 字面

**重构** —— 不做

**测试 / gate（S2 17 句字面 + 双重 close 综合 gate）**

```bash
# S2 17 句字面（同上）跑过 → 全 exit 0
# ... (重复上面 17 句字面 bash assertion)

# 累计 v1.0 release readiness gate
bash scripts/v1-acceptance.sh > /tmp/v1-acc-c7-s2.json
test "$?" = '0'
jq -e '.all_ok == "ok"' /tmp/v1-acc-c7-s2.json > /dev/null
jq -e '.total == 7' /tmp/v1-acc-c7-s2.json > /dev/null
jq -e '.sc1_ai_0shot_assets == "ok"' /tmp/v1-acc-c7-s2.json > /dev/null
jq -e '.sc7_plugin_install == "ok"' /tmp/v1-acc-c7-s2.json > /dev/null

# 既有 vitest 不退化
bun x vitest run 2>&1 | grep -qE 'Tests +593 passed'
bun x vitest run 2>&1 | grep -qE 'Test Files +27 passed'
bun run typecheck > /dev/null

# 既有 c[1-5] MCP smoke 不退化
bash scripts/simx-c1-mcp-smoke.sh 2>/dev/null | jq -e '.all_ok == "ok"' > /dev/null
bash scripts/simx-c4-doctor-smoke.sh 2>/dev/null | jq -e '.all_ok == "ok"' > /dev/null
bash scripts/simx-c5-plugin-validate.sh 2>/dev/null | jq -e '.all_ok == "ok"' > /dev/null
bash scripts/simx-c3-ci-matrix-validate.sh 2>/dev/null | jq -e '.all_ok == "ok"' > /dev/null

# 不变量退化检查（CLAUDE.md §9 字面 7 红线 + S1 baseline）
test "$(jq -r '.dependencies | keys | length' package.json)" = '3'  # 0 dep 增
test "$(find src -name '*.test.ts' -not -path '*/node_modules/*' | wc -l | tr -d ' ')" = '27'  # 27 test files 0 增 0 减
test -f .claude-plugin/plugin.json
test -f .github/workflows/ci.yml
test -f src/sim/runner-supervisor.ts
test -f src/cli/commands/doctor-schemas.ts
test -f scripts/v1-acceptance.sh
test -f scripts/simx-c5-plugin-validate.sh

# v1.md §4 7 SC 字面 0 改字面（决策 5.B 字面）
grep -qE 'AI 0-shot.*Claude Code' docs/v1.md
grep -qE 'iOS 17\.5 / 18\.4 / 26\.x' docs/v1.md
grep -qE 'Claude Code 集成.*claude /install simx' docs/v1.md
```

期望：全 exit 0；任一 fail → S2 不通过 / 回到红 phase 重写 docs/ diff。

## Checkpoint C7 验收（机器可判 / 单一脚本 / v0.7 整体 close + v1.0 release ready 综合 gate）

```bash
#!/usr/bin/env bash
set -euo pipefail

# (a) v1-acceptance.sh 真本机跑 + 9 字段 all_ok=ok（v1.0 release readiness 终极 gate）
bash scripts/v1-acceptance.sh > /tmp/v1-acc-c7.json
test "$?" = '0'
test "$(wc -l < /tmp/v1-acc-c7.json | tr -d ' ')" = '1'  # single-line JSON
jq -e '.all_ok == "ok"' /tmp/v1-acc-c7.json > /dev/null
jq -e '.total == 7' /tmp/v1-acc-c7.json > /dev/null
jq -e '.sc1_ai_0shot_assets == "ok"' /tmp/v1-acc-c7.json > /dev/null
jq -e '.sc2_self_correct == "ok"' /tmp/v1-acc-c7.json > /dev/null
jq -e '.sc3_cold_start_under_5s == "ok"' /tmp/v1-acc-c7.json > /dev/null
jq -e '.sc4_tap_under_50ms == "ok"' /tmp/v1-acc-c7.json > /dev/null
jq -e '.sc5_longrun_100 == "ok"' /tmp/v1-acc-c7.json > /dev/null
jq -e '.sc6_runtime_matrix == "ok"' /tmp/v1-acc-c7.json > /dev/null
jq -e '.sc7_plugin_install == "ok"' /tmp/v1-acc-c7.json > /dev/null

# (b) vitest 593 / 27 文件 0 退化
bun x vitest run 2>&1 | grep -qE 'Tests +593 passed'
bun x vitest run 2>&1 | grep -qE 'Test Files +27 passed'
bun run typecheck > /dev/null

# (c) README.md v1.0 release docs 字面契约（S1 13 句 + 综合）
! grep -qE '^v0\.3 — selector resolver' README.md
grep -qE '^\*\*v1\.0\*\*' README.md
grep -qE '593 (vitest|TS|unit)' README.md
grep -qE '27 (MCP )?tool' README.md
test "$(grep -cE '✅' README.md)" -ge '7'
grep -qE 'scripts/v1-acceptance\.sh' README.md
grep -qE '^## Quick start$' README.md
grep -qE 'claude --plugin-dir' README.md
grep -qE '\(\./docs/plugin-install\.md\)' README.md
test "$(grep -cE '\*\*v0\.[4-7]\*\* \(done\)' README.md)" = '4'
! grep -qE '^TBD\.$' README.md
grep -qE '^## License' README.md
grep -qE '^## Authoring guide for AI agents$' README.md
test "$(grep -cE '^### (Selectors — what to use|Selectors — what NOT to use|Actions|Assertions|Things to never do|When a test fails)$' README.md)" = '6'
! grep -qE 'app\.fill \(HID keyboard\) lands in v0\.7\+' README.md

# (d) docs/roadmap.md 翻转字面
grep -qE '^\| \*\*v0\.7\*\* \| ✅ done \|' docs/roadmap.md
grep -qE '^\| \*\*v1\.0\*\* \| ✅ done \|' docs/roadmap.md
! grep -qE '^\| \*\*v0\.7\*\* \| 🔥 hot \|' docs/roadmap.md
! grep -qE '^\| \*\*v1\.0\*\* \| ❄️ cold \|' docs/roadmap.md
grep -qE 'v1 cycle ✅ 完结' docs/roadmap.md
grep -qE 'v0\.7-c7-hot\.md' docs/roadmap.md

# (e) docs/v1.md 决策日志 +2 行字面
test "$(grep -cE '^- \*\*2026-05-16\*\* v0\.7 C7 close' docs/v1.md)" = '1'
test "$(grep -cE '^- \*\*2026-05-16\*\* v1\.0 整体 close' docs/v1.md)" = '1'
grep -qE 'v1 cycle 完整签名' docs/v1.md
test "$(wc -l < docs/v1.md | tr -d ' ')" = '179'  # 177 + 2

# (f) docs/plugin-install.md 字面 v1.0 release 措辞
grep -qE 'v1\.0 release' docs/plugin-install.md
grep -qE '27 tools' docs/plugin-install.md

# (g) v1.md §1-§6 边界字面 0 改保镖
grep -qE 'AI 0-shot.*Claude Code' docs/v1.md
grep -qE 'iOS 17\.5 / 18\.4 / 26\.x' docs/v1.md
grep -qE 'Claude Code 集成.*claude /install simx' docs/v1.md
grep -qE '冷启动延迟.*< 5s' docs/v1.md
grep -qE '单 tap 延迟.*< 50ms' docs/v1.md

# (h) 既有 c[1-5] smoke 不退化
bash scripts/simx-c1-mcp-smoke.sh 2>/dev/null | jq -e '.all_ok == "ok"' > /dev/null
bash scripts/simx-c4-doctor-smoke.sh 2>/dev/null | jq -e '.all_ok == "ok"' > /dev/null
bash scripts/simx-c5-plugin-validate.sh 2>/dev/null | jq -e '.all_ok == "ok"' > /dev/null
bash scripts/simx-c3-ci-matrix-validate.sh 2>/dev/null | jq -e '.all_ok == "ok"' > /dev/null

# (i) 不变量退化检查（CLAUDE.md §9 字面 7 红线 + scope ≤ 6 文件）
test "$(jq -r '.dependencies | keys | length' package.json)" = '3'                                # 0 dep 增
test "$(find src -name '*.test.ts' -not -path '*/node_modules/*' | wc -l | tr -d ' ')" = '27'   # 27 test files
test "$(find scripts -maxdepth 1 -name 'simx-*' | wc -l | tr -d ' ')" -ge '31'                   # 既有 31+ simx-* scripts 0 减
test -f .claude-plugin/plugin.json
test -f .github/workflows/ci.yml
test -f src/sim/runner-supervisor.ts
test -f src/cli/commands/doctor-schemas.ts
test -f scripts/simx-c5-plugin-validate.sh
test -f scripts/v1-acceptance.sh
test -f src/__tests__/v1-acceptance-shape.test.ts
test -f examples/login-tap.test.ts                  # SC[1] evidence
test -f examples/tap-text-selector.test.ts          # v0.2 evidence
test -f scripts/mcp-smoke.ts                        # v0.6 e2e

# (j) roadmap.md ✅ done 行计 ≥ 9（v0/v0.1-v0.7 + v1.0 = 9 行）
test "$(grep -cE '✅ done' docs/roadmap.md)" -ge '9'

# (k) 4 层信息架构字面在场（CLAUDE.md §0 不变量 §9.7 字面）
test -f docs/roadmap.md        # [1/4]
test -f docs/v1.md             # [2/4]
test -f docs/plan-hot.md       # [3/4]，归档前
test -f docs/plan-cold/v0.7.md # [4/4]
test "$(find docs/plan-cold -maxdepth 1 -name 'v0.*.md' | wc -l | tr -d ' ')" = '7'  # v0.1-v0.7 7 文件

echo "C7 acceptance: PASS — v0.7 整体 close + v1.0 release ready 字面达成"
```

期望：上述命令全 exit 0 / 末行字面 `C7 acceptance: PASS — v0.7 整体 close + v1.0 release ready 字面达成`；任一条 fail → C7 未通过、不进入归档动作。

## 完成后动作

1. **归档当前 hot plan**：`mv docs/plan-hot.md docs/plan-history/v0.7-c7-hot.md`（决策 7.A 字面 —— CLAUDE.md §6 标准归档动作字面 mirror v0.7 C[1-6] 6 次归档同模式）；
2. **v1 cycle close 状态字面公示**：v0.7 C7 close + v1.0 整体 close + v1 cycle 完整签名 已字面 commit 到 `docs/v1.md` 决策日志 + `docs/roadmap.md` v0.7 ✅ + v1.0 ✅ + §当前所在位置 段；**不**新建 `docs/v1.0-release-notes.md`（决策 0.B 字面 —— release notes 等价记在 v1.md decision log v0.1-v0.7 7 大段 + roadmap.md 双 SoT）；
3. **下段热化触发字面**：**当且仅当用户/上层 agent 字面说 "开始 v1.1 C1" 或同义时**，调 sub-agent 字面执行（决策 7.B 字面）：
    - 先生成 `docs/plan-cold/v1.1.md` 字面（v1.1 cold plan，CLAUDE.md §3 字面 ≤ 100 行格式 / 范围 = Watch mode + Cell L4 并行调度 + matrix run + Cell L3 TUI 状态行）；
    - 再 main convo sub-agent 字面生成新 `docs/plan-hot.md` 字面（v1.1 C1 范围，CLAUDE.md §2 + §6 标准热化模板字面）；
    - 起点 baseline 字面 = TS 593 / 27 test file / 102 swift / 27 MCP tool / 3 prod dep / 31+ scripts/simx-* + v1-acceptance.sh / 6-check doctor compat supported / 9-field c4 doctor smoke / 11-field c5 plugin-validate / 14-field c3 ci-matrix-validate / 9-field v1-acceptance.sh / `.claude-plugin/plugin.json` / `.github/workflows/ci.yml` 3 branch matrix / `docs/plan-cold/v0.[1-7].md` 7 文件 / `docs/plan-history/v0.[1-7]-c[1-N]-hot.md` 30+ 归档 / `docs/{roadmap,v1,design,plugin-install}.md` / README.md v1.0 release docs / examples/ 3 真测试 + 3 子目录 / src/{core,sdk,driver,mcp,sim,cli} 全模块；
4. **下段热化前置检字面**（CLAUDE.md §6 字面）：`test -f docs/plan-history/v0.7-c7-hot.md` + `grep '^- \*\*2026-05-16\*\* v1\.0 整体 close' docs/v1.md` + `grep '^\| \*\*v1\.0\*\* \| ✅ done' docs/roadmap.md` + `bash scripts/v1-acceptance.sh | jq -e '.all_ok == "ok"'` + `bun x vitest run | grep 'Tests +593 passed'` 全 exit 0；
5. **v1.0 release 真发布动作字面**（**不在 C7 范围**，推 v1.1 C0 / pre-v1.1 人工运维窗口）：
    - (a) `git init` + `git remote add origin <real-repo>` + 首次 push GitHub repo；
    - (b) 真 repo URL 替换 `.claude-plugin/plugin.json` line 5-6 字面 `homepage` / `repository` 占位 `anthropic-experimental/simx` → 真 URL；
    - (c) 真 repo URL 替换 `docs/plugin-install.md` §4 字面占位提示；
    - (d) 真 GHA CI matrix run 远端 verify iOS 17.5/18.4/26.4 三 runtime；
    - (e) iOS 17/18 5-arg HID 真路径实装窗口（v0.7+ 范围 / cold plan v0.7 §知名风险字面延续）；
    - (f) `claude --plugin-dir <abs>` install + session 内 `/mcp` 真 enumerate 27 tool（docs/plugin-install.md §3 字面人工 verify 路径）；
    - (g) npm registry publish + Claude Code marketplace submit（v1.0+ 范围 / docs/plugin-install.md §4 字面延续）；
6. **CLAUDE.md §9 不变量字面再确认**：v0.7 C7 close 后 7 红线字面 0 违反——(1) 只支持模拟器 ✅ / (2) 不引入 multi-provider VLM 抽象（explain_screen 走本机 claude CLI）✅ / (3) 不暴露 xpath / 坐标 selector 到 DSL 表面 ✅ / (4) 不提供裸 sleep API ✅ / (5) 失败信息必须 AI-readable（含 visibleElements / suggestions）✅ / (6) 私有符号必须 dlsym 动态加载 ✅ / (7) 4 层信息结构始终保持 ✅（C7 close 时本 plan-hot 归档 + roadmap.md / v1.md / plan-cold/v0.[1-7].md 全在场）。

## 与 cold plan 偏差汇总（**必读 - 不要隐瞒**，决策 8 系列字面）

| 维度 | cold plan v0.7.md / cold plan §C7 字面 | C7 hot plan 实际落地 | 决策记号 |
|---|---|---|---|
| README rewrite 形态 | "README / docs/ 更新到 v1.0 发布状态" 字面（未明指全 rewrite vs incremental） | incremental update（候选 C 字面）—— 5 段替换 + 1 行删 + §Authoring / §Why / §src tree / §Example 主体 0 改 / 单文件 ≤ 80 行 diff | 0.A |
| 全 rewrite 路径 | — | 字面拒绝（候选 A）：§Authoring 段 v0.3 起即 SC[1] evidence 字面在场 + 推翻 = 高 regression risk + 文件 ≤ 6 约束下不必要 | 0.A |
| v1.0 release-notes.md | — | **不**新建（决策 0.B）—— release notes 等价记在 v1.md decision log v0.1-v0.7 7 大段 + roadmap.md 双 SoT | 0.B |
| Authoring guide 形态 | "Authoring guide（cold plan §SC[1] 字面要求）" + "docs/authoring.md 或 README section" | §Authoring guide for AI agents 段 0 改字面（README line 89-200 是 v1.0 终态）+ **不**新建 docs/authoring.md（双 SoT 风险 + overlap） | 0.C |
| 27 tool 表呈现 | "27 tool 表" 字面 | README §Status 段表格 1 行 categories（1 行字面）/ 不逐 tool 列名（避免 SoT 漂移 vs src/mcp/tools.ts） | 0.D |
| Success Criteria 状态表 | "Success Criteria 状态" 字面 | 表格 7 行 SC[1]-[7] / 全 ✅ / Evidence 字段名映射到 v1-acceptance.sh 9 字段（不含 total / all_ok） | 0.E |
| Quick start 形态 | "Quick start (claude --plugin-dir / npm install future / git clone)" 字面 | 3 路径段（claude --plugin-dir / git clone+bun install / npm install placeholder）+ repo URL 占位字面 1 行 hint | 0.F |
| Plugin install link | "Plugin install (link 到 docs/plugin-install.md)" 字面 | README §Status / §Quick start 段双链接到 docs/plugin-install.md | 0.F |
| 链接到 GitHub repo（占位） | "链接到 GitHub repo（占位）" 字面 | repo URL `anthropic-experimental/simx` 占位字面延续在 .claude-plugin/plugin.json + docs/plugin-install.md / README §Quick start 段加 1 行 release-time hint 字面 | 8.A |
| dev-sim lock 说明 | "dev-sim lock 说明" 字面 | §Local dev §Dev simulator 段（line 219-229）字面 0 改字面（v0.3 起 .simx/dev-sim.txt 流程已字面正确） | 决策保 §0 改 |
| License + contributing | "License + contributing" 字面 | §License "TBD" → "MIT" / contributing 段**不**新建（决策 6.B / 决策 8.E 字面 ——contributing 字面推 v1.0+ post-release，CLAUDE.md §11 字面 simx 是 Claude Code 子产品已隐式说明 contribution 路径） | 6.B / 8.E |
| docs/design.md v1.0 标记 | "docs/design.md 是否要 v1.0 标记" 候选 | **不**改 design.md（决策 8.F）—— design.md 是 why 决策 SoT，与 status 标记跨层；v1.0 标记字面在 v1.md decision log + roadmap.md ✅ done 双 SoT 已覆盖 | 8.F |
| 测试覆盖（vitest case） | "vitest case？" / "或加 scripts/simx-c7-readme-validate.sh grep 关键 section 在场" 候选 | **不**挂 vitest case（决策 4.A）/ **不**新建 simx-c7-readme-validate.sh（决策 4.B 字面 —— shell-level 13+17 句字面 contract gate 嵌入 S1/S2 / Checkpoint C7 验收脚本 = 32 句字面 + v1-acceptance.sh wrap = 综合 gate） | 4.A / 4.B |
| roadmap.md 翻转 | "v0.7 🔥 → ✅" + "v1.0 ❄️ → ✅" + "当前所在位置 → v1.1 或留空（v1 cycle close）" 字面 | 全字面落地 + §当前所在位置 5 → 4 行 rewrite（v1 cycle ✅ 完结 / v0.7 C1-C7 全 close / v1.1 下一步 / plan-hot 归档 v0.7-c7-hot.md） | 1.B / 11.A |
| plan-history v0.7-c7-hot.md | "plan-history v0.7-c7-hot.md" 字面 | §完成后动作 §1 字面 `mv docs/plan-hot.md docs/plan-history/v0.7-c7-hot.md`（CLAUDE.md §6 字面标准归档动作） | 7.A |
| v1.md 决策日志最终 2 行 | "v0.7 C7 close" + "v1.0 整体 close + v1 cycle close + 7 SC 全过" 字面 | +2 行字面落地（详尽内容见 §决策 1.C 字面）/ v1.md §1-§6 边界字面 0 改保镖（决策 5.B 字面延续） | 1.C / 5.B |
| docs/plugin-install.md 改 | "docs/plugin-install.md (link 到)" 字面 | line 4-5 字面微调（v0.7 C5 落地形态 → v1.0 release 落地形态 / forward-looking → done-state）+ §3 / §4 / repo URL 占位 0 改字面 | 1.A |
| v1.1 cold plan 新建 | — | **不**新建 docs/plan-cold/v1.1.md（推 main convo 调度 sub-agent，CLAUDE.md §6 字面）—— C7 范围是 v0.7 close + v1.0 release，**不**预先字面 v1.1 范围（避免冷热越层 / CLAUDE.md §1 字面冷热分离） | 7.B |
| 文件改动数 | "≤ 6 文件改动" 字面 | 4 文件改字面（README.md + docs/plugin-install.md + docs/roadmap.md + docs/v1.md）+ 1 归档（plan-hot → plan-history/v0.7-c7-hot.md）= 5 文件级动作 ≤ 6 合规 | scope |
| 步骤数 | "1-3 step" 字面 | 2 step（S1 README rewrite + S2 docs/ 双重 close 仪式）合规 | scope |
| TS / Driver / Cell / dep | "swift 0 / Driver 0 / Cell 0 / 0 新 dep" 字面 | 0 swift / 0 Driver / 0 SimctlDriver / 0 Cell / 0 SimctlClient / 0 dep / 0 既有 27 ToolDef / 0 既有 27 test file / 0 src/ 改 / 0 examples 改 | 不变量 |
| v1.1 任何东西 | "不实现 v1.1 任何东西（cell L4 / parallel / watch / matrix run）" 字面 | 0 实现（仅 README §Roadmap 段 + roadmap.md §v1 之后表格字面 v1.1 名字 prefetch，**不**字面新建 v1.1 code / cold plan / hot plan） | 不变量 |
| 既有 acceptance / vitest | "不破 既有 acceptance / 既有 vitest" 字面 | v1-acceptance.sh + 6 MCP smoke + c4-doctor / c5-plugin / c3-ci-matrix-validate + vitest 593 / 27 文件全 0 退化字面（C7 验收 §a / §b / §h 字面 wrap） | 不变量 |
| partial state 字面延续 | C6 字面 partial-as-ok 解读 | C7 README §Status 状态表全 ✅ + sc[3]/sc[6] partial 含义记在 v1.md decision log + C7 close 决策日志再 reiterate + 不引入"partial" 字面到 README 状态表 | 9.B |

**决策 8.A 字面 —— repo URL 占位**：cold plan §C7 字面 "链接到 GitHub repo（占位）"；C7 落地 = 占位 URL `https://github.com/anthropic-experimental/simx` 字面**保留** in `.claude-plugin/plugin.json` line 5-6 + `docs/plugin-install.md` line 64；README §Quick start 段加 1 行字面 release-time hint `_Note: the homepage/repository fields in .claude-plugin/plugin.json and docs/plugin-install.md currently hold the placeholder URL; replace with the real repo URL on first GitHub push._`；真 URL 替换字面推 v1.1 C0 / pre-v1.1 人工运维（git push GitHub 时一次性替换 3 处字面）= 决策 7.B / §完成后动作 §5 字面延续。

**决策 8.B 字面 —— npm publish 推 v1.0+**：cold plan §C7 字面 "Quick start (claude --plugin-dir / npm install future / git clone)"；C7 落地 = npm install 路径字面是 placeholder（"_Coming v1.0+: marketplace publish, see docs/plugin-install.md §4_" 1 行字面 / 决策 0.F 字面 / docs/plugin-install.md §4 字面已是 placeholder 段，C7 0 改字面延续）；真 npm publish 字面推 v1.0+ post-release / CLAUDE.md §11 字面 simx 是 Claude Code 子产品分发优先 Claude Code MCP plugin path / npm 是次要 path。

**决策 8.C 字面 —— claude install 真 verify 推 manual**：cold plan §SC[7] 字面 "claude /install simx" → v0.7 C5 close 字面修正为 `claude --plugin-dir`（plugin-install.md SoT）；C7 落地 = README §Quick start 段字面 `claude --plugin-dir /absolute/path/to/simx` + 链接到 docs/plugin-install.md 字面（pipe 不可 enumerate 27 tool 字面在 plugin-install.md §3 字面已说明）；真 install + session `/mcp` enumerate 真 verify 字面推 v1.0 release time 人工 verify / docs/plugin-install.md §3 字面延续。

**决策 8.D 字面 —— cold plan v0.7.md 0 改延续**：cold plan v0.7.md 字面是 v0.7 入口假设 SoT；C7 范围内 0 改字面（决策日志解释偏差 + cold plan 留作历史）—— 决策延续 v0.7 C1-C6 6 次 hot plan 同模式（0 改 cold plan v0.7）；v1.1 cold plan 字面**新建**字面推 main convo 调度 sub-agent（CLAUDE.md §6）；2 cycle 边界过渡字面**不**在 C7 内（决策 7.B）。

**决策 8.E 字面 —— contributing 段不新建**：cold plan §C7 字面 "License + contributing" 候选；C7 落地 = License 段字面 TBD → MIT；contributing 段**不**新建（理由：CLAUDE.md §11 字面 simx 是 Claude Code 子产品 / 隐式 contribution 路径 / 文件 ≤ 6 约束下避免额外字面）；contributing 段字面推 v1.0+ post-release 时一次性补完 / CONTRIBUTING.md 字面是常见 GitHub 仪式可在 git push GitHub 时一次性建（决策 7.B § (a) 字面延续）。

**决策 8.F 字面 —— docs/design.md 0 改延续**：cold plan §C7 字面 "docs/design.md 是否要 v1.0 标记" 候选；C7 落地 = 0 改 design.md（决策延续 v1 cycle 全期 0 改 design.md 字面）——design.md 是 why 决策 SoT、与 status 标记跨层；v1.0 release 状态字面在 v1.md decision log + roadmap.md ✅ done + README §Status 段三 SoT 已覆盖；design.md 字面是 v0/v0.1/v0.2/v0.3 设计期落地 + 后续 v0.4-v0.7 决策日志记在 v1.md / 不渗到 design.md（避免跨层混用 / CLAUDE.md §0 字面）。

**决策 9.A 字面 —— v1.0 release notes 等价 SoT**：v1.md decision log v0.1-v0.7 7 大段字面（每段 1500+ 字 / 累计 7 段 86+ 行字面）= fully fleshed-out release notes 字面 SoT；roadmap.md ✅ done 行字面是 high-level 一句话；README §Status 状态表 + Baseline 表是 user-facing 简明版；3 SoT 字面互不重叠 + 不引入第 4 SoT release-notes.md（决策 0.B 字面延续）。

**决策 9.B 字面 —— partial state README 呈现**：v0.7 C6 close 决策日志字面已说明 SC[3]/SC[6] partial state 含义（cold plan 字面 "[3] 待 v0.7 长跑稳定验" + "[6] 推 v0.7 CI 矩阵 / iOS 17/18 真路径未实装" / v1-acceptance.sh 字段 binary `ok` 字面）；C7 README §Status 状态表落地 = 全 ✅ 字面 / 不引入"partial" 字面到 user-facing；partial 解读字面 SoT 记在 v1.md decision log + C7 close 决策日志再 reiterate；real status closure 字面（iOS 17/18 5-arg HID 真路径实装 + 远端 GHA replay）字面推 v1.1 / v1.0+ release time 人工 verify（决策 7.B § (d) / § (e) 字面延续）。

**决策 10.A 字面 —— CLAUDE.md §4 TDD 三段红绿重构 适用 C7**：C7 是文档收口、0 src/ / 0 vitest；红绿三段在 C7 字面落地形态 = 红 phase = bash assertion 字面契约 13+17 句先 fail / 绿 phase = README + docs/ 4 文件字面落地满足 contract / 重构 phase = N/A（决策 5.A 字面 / 文档改不重构）；CLAUDE.md §4 字面"测试先于实现写"在 C7 等价于"字面契约 gate 先于 README/docs/ 改"—— S1/S2 字面已先列 13+17 句 bash contract 再写绿 phase 改动；CLAUDE.md §4 字面"测试**必须先失败一次**"在 C7 等价于"bash assertion 当前实测必 fail 至少 1 句"——S1 红 (1)/(3) 字面 + S2 红 (4)/(11)/(12)/(15) 字面已实测先 fail；契约 gate 在 S1/S2 / Checkpoint C7 验收三处嵌入字面 = 测试三段位置不变量。

**决策 11.A 字面 —— roadmap.md §当前所在位置 段 rewrite 形态**：v1 cycle close 后字面 SoT 形态：(a) 大版本字段 "v1 cycle ✅ 完结" 字面（用户任务字面 候选 "v1.1 或留空（v1 cycle close）"——选 "v1 cycle ✅ 完结" 而非 "留空"，理由：明确字面 SoT > 留白 + 等价 v1.0 release ready 状态）；(b) 当前状态字段细列 v0.7 C1-C7 7 次 close 字面（明确字面 SoT > "v0.7 全 close" 简单字面）；(c) 下一步字段 = v1.1 字面 + cold plan v1.1.md 待生成提示（决策 7.B 字面）；(d) plan-hot 状态字段 = 归档到 v0.7-c7-hot.md 字面（决策 7.A 字面）；4 行字面 mirror roadmap.md 原 5 行字面同结构（CLAUDE.md §0 字面 4 层信息架构 = roadmap.md 是 [1/4] 元状态 SoT）。

**决策 12.A 字面 —— C7 命名 = README docs/ 收口字面**：C7 hot plan 标题字面"README / docs/ 更新到 v1.0 发布状态 + 双重 close"——mirror cold plan §C7 字面 + 双重 close 仪式字面（v0.7 🔥→✅ + v1.0 ❄️→✅）；不命名为 "simx-c7-readme-validate.sh" 字面（决策 4.B 字面 / 不新建 shell script / 字面契约 gate 嵌入 hot plan 本身）；C7 是 v1.0 release checkpoint 命名级别（mirror v06/v05/v04 acceptance 字面 version-level，但 C7 是 README+docs 收口、不是 acceptance level）。

**决策 15.B 字面 —— cold plan v0.7.md 字面 0 改延续 + 文档生命周期**：cold plan v0.7.md 字面是 v0.7 进入时假设 SoT、留作历史；C1-C7 7 次 hot plan 累积偏差全记决策日志 / cold plan 字面 0 改；C7 close 时 cold plan v0.7.md 字面**保留**作 v0.7 cycle 历史快照（不归档到 plan-history、不 mv 到别处）—— 与 plan-hot.md 归档到 plan-history 形成对比：plan-hot 是 [3/4] 信息层即一刻状态、寿命 1 个 checkpoint 故归档；cold plan v0.X.md 是 [4/4] 信息层即一个 minor 版本概要、寿命 1 个版本故保留（CLAUDE.md §0 字面）；plan-history 段累计 v0.[1-7]-c[1-N]-hot.md 30+ 文件字面是 v1 cycle 全期决策追溯路径 SoT。

> END plan-hot v0.7 C7 — v0.7 整体出口 / v1.0 release readiness / v1 cycle 完整签名 三重 milestone 在 C7 close 时一次性 commit；plan-hot.md 自归档到 plan-history/v0.7-c7-hot.md；v1.1 cold plan + v1.1 hot plan 字面待 main convo 调度 sub-agent 在用户字面说"开始 v1.1 C1"后生成；v1 cycle 在 lab15-autofix 项目 4 层信息架构下完整执行 + 全决策日志可追溯 + 全 checkpoint 机器可判 + 全 TDD 三段红绿重构 + 全冷热分离 + 全 0 不变量违反。
