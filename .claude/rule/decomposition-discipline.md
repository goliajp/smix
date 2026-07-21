# debug/* — Decomposition discipline

> 提炼自 SPG 项目 v7.37 ship 阶段的 perf 方法论复盘 (2026-06-21), 该文档在本仓库之外.
> 适用 capability 调试 / perf attack / 行为不通 dig — 不限性能.
>
> smix v5.12 c1 swift `/tap-by-id` handler 4 轮试错 (commit 92816da) 是反面教材: 第 2 轮没动针就该 STOP 切 decomposition.
>
> 字段同 `~/.claude-shared/global/principles.md` 的 rule 卡片 schema.

---

## debug/two-round-stop

- **Treats** — debug workflow (capability / perf / behavior dig)
- **Rule** — 同一目标做 2 轮 polish (改一刀 + 跑试) 仍没动针 (功能没通 / 行为没改 / 数据没动) → 立刻 STOP. 第 3 轮**禁止**继续 polish; 必须切 read-only decomposition 模式.
- **Triggers** — 单 session 内对同一 capability / 同一 fail step / 同一 perf endpoint 已 attempt ≥2 次代码修改, 验证结果相同 (依然 fail / 依然 sub-noise / 依然 stale). 第 3 次开始改之前 = 违反.
- **Why** — polish 永远没动针 = 在攻击不是真瓶颈/真根因的位置. 继续 polish 只是更精细地猜. SPG SCALARSQ 浪费 10 轮 polish 后被 decomposition 3 commits 闭红线 -85%; smix v5.12 c1 swift handler 4 轮试错 (XCUI coord.tap → element.tap → IOHID stale-frame → IOHID snapshot+settle) 后才意识到根因是 "SDK 走 IOHID daemonProxy / swift handler 走 XCUI gesture chain" — decomposition 一对比就 surface, 不需 4 轮.
- **Bad** — "再试加个 sleep / 再换个 API / 再 cycle 一次 capsule 看 stderr"
- **Good** — "2 轮没动针, STOP. 派 Explore agent 读 SDK 同类工作 path 完整调用链 + 我自家 fail path 完整调用链, side-by-side 拆 stages with file:line + atomic op. Ground truth 出来再动手."
- **Exceptions** — flaky environment 验证 (eg sim socket EOF 重连), 不是 polish; 重跑确认是环境而非代码.
- **See also** — `debug/no-ceiling-words`, `debug/decomposition-before-attack`, `[[agent-research-verify-before-implement]]`, `[[guideline-find-root-cause-first]]`

---

## debug/no-ceiling-words

- **Treats** — response wording (in debug deliberation)
- **Rule** — 以下话术出现在 debug 推理中 = 红灯, 立刻怀疑 decomposition 不够细, 强制停下来问 "我 file:line 拆够了吗?". 这些词**可能**真 (e.g. v5.x-backlog-c4 modal binding 确是 iOS 平台 bug), 但 ≥9/10 是 self-deception 包装.
- **Triggers** — response / 思考链含以下任一: "iOS 平台 bug" / "XCUI 限制" / "Apple 不开放" / "snapshot stale 是 platform behavior" / "swipe inertial momentum 不可控" / "等 Apple FB" / "结构性 gap" / "language ceiling" / "architectural ceiling" / "user-space 不可触" / "sub-bench noise" / "syscall / kernel 残余" / "v5.x-backlog-c{N} 平台级" — 且尚未做过 read-only decomposition.
- **Why** — 这些词包装 "我懒得继续找了". 真平台限制必须**先**有 decomposition 文档证明 (对手在同一路径上也跑不通 / 对手用同一 API 也 fail / disasm 显示对手不绕过同一 selector) 才能 claim. 没 ground truth 直接说 "platform ceiling" = 早 abort 的借口.
- **Bad** — "看来是 iOS 17+ SwiftUI binding 不响应 XCUI tap, 这是平台限制" (没读 SDK 同类 handler 的 IOHID 路径前)
- **Good** — "怀疑是 platform 限制, 但没 grep SDK 工作的同类 tap path 验证. 派 Explore 读 SDK smix tap 完整链 + swift /tap-at-norm-coord handler vs /tap-by-id handler 实现对比, 看 SDK 有没有走不同 API. 1 hr 后再 claim platform limit."
- **Exceptions** — decomposition 已做且 ground truth 文档显示对手同源失败 → claim platform limit 合法.
- **See also** — `debug/two-round-stop`, `debug/decomposition-before-attack`

---

## debug/decomposition-before-attack

- **Treats** — debug workflow phase separation
- **Rule** — 不通 / 不动针的 capability / perf 调试 = 两段 dance: (1) Decomposition phase = **read-only** research, 拆 N stages × file:line × atomic-op (或 selector / API call / data flow), side-by-side 对手 vs 自己, 产 ground truth 文档. (2) Attack phase = 基于 ground truth atomic 实施一刀, validate. 两段**不混**.
- **Triggers** — 单个 agent / 单段对话内出现: 同时 read + write + run + read stderr + 改回; 或 "我试一下 X" 切换 ≥2 次假设没产文档. 这种边读边改边试的 spin 模式 = 违反.
- **Why** — 一个 agent 同时读 + 改会被 build error 拉走, 被 build 时间消耗 patience, 半路 doubt 切换假设, 最后产出 "试了 X 但没用" 总结, 跟反复 polish 没区别. 职责隔离: read-only agent 不被改代码诱惑 + 不被 "我先试一下" 拉走; write agent 不需 re-derive attribution.
- **Bad** — "我 grep 一下 + 改 swift handler + 重启 capsule + 跑 yaml + 看 stderr + 改回再试..." (1 hr 内 4 轮试错没产文档)
- **Good** — "先 Explore agent (read-only) 拆 SDK smix tap path 完整调用链 + swift /tap-by-id handler 完整调用链 file:line, 对比每段 atomic op (BTreeMap descent / dispatch / IOHID synthesize vs XCUI gesture chain) 产 ground truth 文档. 文档出来后 attack agent atomic 改一刀 + 一次 capsule restart + 跑 yaml validate."
- **Exceptions** — 已知确定性根因的 1-LOC 改动 (例: bump 一个常量, 改一个 typo); 这类不需 decomposition, 直接改.
- **See also** — `debug/two-round-stop`, `debug/no-ceiling-words`, `[[smix-must-be-superset-of-maestro]]` (smix 的"对手" = maestro CLI + SDK 自家工作 path + 现有同类 handler), `[[agent-research-verify-before-implement]]`

---

## 适用范围 / scope notes

- **不限 perf**. 文章侧重 perf attack, 但 decomposition + 2-round-stop 原则适用任何 "改一刀验一次" 的调试场景 (capability 不通 / e2e fail / 行为偏差 / API 兼容不通 etc).
- **对手 = comparison target**, 不限竞品. smix 的 "对手" 在不同 context 可以是: maestro CLI (cross-product comparison) / SDK smix tap path (cross-handler comparison) / WebDriverAgent (cross-toolkit comparison) / 已 PASS 的同类 yaml (cross-fixture comparison).
- **atomic op 表参考** 同 SPG 文章 §6 (Apple Silicon M-series 基准), smix 项目 swift sim-side 调用代价同表; 跨进程 XCUI snapshot 额外加 ~1-3ms (modal context 下), 实测见 [[smix-modal-snapshot-sensing]].
- 全局原则 `~/.claude-shared/global/principles.md` 的 guidelines (`guideline/find-root-cause-first` / `guideline/explore-vs-verify` / `guideline/hard-means-do`) 是这三条 rule 的心智源头; rule 是机械触发, guideline 是姿态.
