# Roadmap — [1/4]

> 全版本路径。一行一句。详细范围在 `v{cur}.md`，详细计划在 `plan-cold/v0.X.md`，下一步在 `plan-hot.md`。

## v1 cycle（当前）

| 版本 | 状态 | 一句话目标 |
|---|---|---|
| **v0** | ✅ done | TypeScript 骨架 + 设计文档（types-only，driver 是 stub） |
| **v0.1** | ✅ done | simctl wrapper 通电：SDK 能 launch app + screenshot 端到端 |
| **v0.2** | ✅ done | HID 注入层（iOS 17/18/26 三套 Indigo ABI + XCUITest tap fallback） |
| **v0.3** | ✅ done | AX 读取（host-side `AccessibilityPlatformTranslation` + XCUITest snapshot fallback）+ selector resolver |
| **v0.4** | ✅ done | SDK 行为完备：matcher 失败上下文真填充 + `.simx/trace/` 输出 + `waitFor` 真生效 |
| **v0.5** | ✅ done | CLI `repl` + `doctor` 完整版（探测 Xcode / runtime / 私有符号 / claude CLI） |
| **v0.6** | ✅ done | MCP server 18+ 工具（实际 27 expose 含 ping），含 `explain_screen` 走本机 `claude` CLI |
| **v0.7** | ✅ done | Hardening：长跑稳定（每 50 case 重启 runner）+ CI 三 runtime 矩阵 + Claude Code plugin 分发 |
| **v1.0** | ✅ done | v1.md 全部 Success Criteria 通过 + 发布文档 |

## v1 之后

| 版本 | 一句话目标 |
|---|---|
| v1.1 | Watch mode + **Cell L4 并行调度 + matrix run**（多 Cell 同时跑、变体决策分支）+ Cell L3 TUI 状态行 |
| v2 | 完整录制器（AX 事件流监听 + 模板生成）+ Vision OCR 视觉兜底 + Foundation Models 端上推理 + snapshot diff + 寄生 Xcode 26 Automation Explorer + **Cell L3 内嵌 frame streaming viewer** |

## 永不做

- 真机
- Multi-provider VLM 抽象
- xpath / 坐标 selector

## 当前所在位置

- **大版本**：v1 cycle ✅ 完结（边界见 `docs/v1.md` / 7 SC 全过 evidence `bash scripts/v1-acceptance.sh` 9 字段 `all_ok=ok`）
- **当前状态**：v0.7 C1-C7 全 close（C1 长跑稳定 / C2 CI workflow / C3 三 runtime 矩阵 / C4 doctor compatibility / C5 plugin manifest / C6 v1-acceptance.sh / C7 README docs/ 收口）+ v1.0 release ready
- **下一步**：v1.1（Watch mode + Cell L4 并行调度 + matrix run + Cell L3 TUI），cold plan `docs/plan-cold/v1.1.md` 字面待 main convo 调度 sub-agent 生成（CLAUDE.md §6 字面）
- **plan-hot 状态**：v0.7 C7 close 时归档到 `docs/plan-history/v0.7-c7-hot.md`；新 plan-hot 字面待 v1.1 C1 热化触发
