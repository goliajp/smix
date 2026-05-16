# plan-hot — (待 main convo 拍板下一段)

> [3/4 信息层] **transient placeholder**。CLAUDE.md §9 不变量 #7 要求"任何时间点 4 层架构在场"——上一段已 close、下一段未拍板时用此 placeholder 占位，**不**含 step 级内容。
>
> **不要按此文件执行**。需要展开下一段时，main convo 调 sub-agent 按 CLAUDE.md §6 热化模板生成真 plan-hot.md（覆盖此文件）。

## 上一段已 close

- **web v0.1 C1** — simx.golia.jp 占位单页 + Caddy SPA + ACME HTTPS 上线（2026-05-16）
- 归档：`docs/plan-history/web-v0.1-c1-hot.md`
- 验证三条全过：dig A → 18.179.107.143 / HTTP/2 200 / SPA fallback 命中

## 候选下一段（等用户开口选）

| 候选 | 范围 | 冷计划 |
|---|---|---|
| **A. web v0.2** | docs framework + 内容（Authoring guide / Plugin install / 27-tool reference / Examples 4 子页 + sidebar/topbar 路由 + MDX 加载） | 需新建 `docs/plan-cold/web-v0.2.md`（评估"starter+MDX 自实现" vs "切 vitepress"） |
| **B. simx 主项目 v1.1** | Watch mode + Cell L4 并行调度 + matrix run + Cell L3 TUI 状态行（`docs/roadmap.md` §v1 之后字面） | 需新建 `docs/plan-cold/v1.1.md` |
| **C. v1.0 release hygiene** | branch protection / repo topics / about section / first release note / iOS 17/18 5-arg HID 真路径（v0.7 placeholder） | 不是新 minor，散乱任务；不走完整 plan-cold |

## 热化前置条件

CLAUDE.md §6 拒绝条件——若进入 sub-agent 热化时发现以下任一，回报上层不展开：

- 入口条件未满足
- 本机探测与冷计划假设不符
- 当前 v1.md 边界与冷计划范围冲突
- 上一段有未关闭 known issue 影响本段（web v0.1 暂无）
