# plan-hot — (待 main convo 拍板下一段)

> [3/4 信息层] **transient placeholder**。CLAUDE.md §9 不变量 #7 要求"任何时间点 4 层架构在场"——web v0.2 三个 checkpoint 都已 close、下一段未拍板时用此 placeholder 占位，**不**含 step 级内容。
>
> **不要按此文件执行**。需要展开下一段时，main convo 调 sub-agent 按 CLAUDE.md §6 热化模板生成真 plan-hot.md（覆盖此文件）。

## 上一段已 close

- **web v0.2 C1**（2026-05-16）— MDX framework 骨架：vite plugin + sidebar/topbar route + dark mode + 1 占位 page + nav.config + 404；归档 `plan-history/web-v0.2-c1-hot.md`
- **web v0.2 C2**（2026-05-16）— 4 主页真内容 + 27-tool build-time generator + home internal-link CTA；归档 `plan-history/web-v0.2-c2-hot.md`
- **web v0.2 C3**（2026-05-16）— examples 段（索引 + 3 sub-page）+ `@shikijs/rehype` build-time 高亮；归档 `plan-history/web-v0.2-c3-hot.md`

web v0.2 全部 close。

## 候选下一段

| 候选 | 范围 | 冷计划 |
|---|---|---|
| **A. web v0.3** | screencast / asciinema demo + 可能的 docs↔repo 自动 sync 脚本 + search | 需新建 `docs/plan-cold/web-v0.3.md` |
| **B. simx 主项目 v1.1** | Watch mode + Cell L4 并行调度 + matrix run + Cell L3 TUI（`docs/roadmap.md` §v1 之后字面） | 需新建 `docs/plan-cold/v1.1.md` |
| **C. v1.0 release hygiene** | iOS 17/18 5-arg HID 真路径实装（v0.7 placeholder）/ repo topics / about / first release note | 散乱任务；不走完整 plan-cold |

## 热化前置条件

CLAUDE.md §6 拒绝条件——若进入 sub-agent 热化时发现以下任一，回报上层不展开：

- 入口条件未满足
- 本机探测与冷计划假设不符
- 当前 v1.md 边界与冷计划范围冲突
- 上一段有未关闭 known issue 影响本段
