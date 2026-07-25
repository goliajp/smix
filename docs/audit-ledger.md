# audit-ledger — 2026-07-19 那条待办的实证状态

这张表是 `docs/v2.md` 决策日志 2026-07-19「功能闭环审计」末尾那条
「待办(按严重度,未修)」的**当前状态**。那条待办本身是历史,一字不动;
状态活在这里。

**判据栏是可执行的。** `scripts/dev/audit-ledger-scan.py` 每次运行都重新求值每一行的判据,
并把行集合与 v2.md 那条待办里的圈号对账。判据是刻意不对称的:`present` 行钉在**缺陷代码**上
(缺陷被修掉,引文失配,闸门红,逼你改状态);`fixed` 行钉在**修复代码**上(修复被 revert,同样红)。
两个方向都堵住 —— 因为这张表的前身正是在"已修却仍标未修"的方向上烂掉的,3/5 抽验命中。

**改一行时,把最后一列改成当天。** 闸门不检查日期与改动的对应关系(理由写在它的 docstring 里:
那会让人学会无脑改日期),日期的诚实靠人。

| # | 缺陷 | 状态 | 判据 | 可达性 / 理由 | 层 | 核验日 |
|---|---|---|---|---|---|---|
| ① | `localizedText` wire 键三端不一致,两 SDK 的该选择器全 verb 死 | fixed | `at crates/smix-selector/src/lib.rs:387 "serde(rename = "localizedText""` | canonical 键裁决为 camelCase,旧 snake 拼写走 alias;两 SDK 发的正是 camelCase,两条拼法都解得出来 | — | 2026-07-22 |
| ② | `webview_eval` Rust client 硬编码 `127.0.0.1:28080/eval`,与 Kotlin `/webview-eval`(28081 代理)路径端口双错 | fixed | `at crates/smix-runner-client/src/lib.rs:573 "SMIX_WEBVIEW_BRIDGE_PORT"` | 两半分开看:Android 半早已走 `/webview-eval` 代理,不再可达;iOS 直连宿主 loopback **是设计**(07-19 决策日志原文:模拟器共享宿主 loopback)。真缺陷只是端口写死 —— 现默认 28080,可由 `SMIX_WEBVIEW_BRIDGE_PORT` 或 builder 覆盖,4 个断言钉住 | — | 2026-07-22 |
| ③ | MCP `ocrText` 全路径恒失败,`assert_not_visible(ocrText)` 恒假绿 | fixed | `at crates/smix-mcp/src/main.rs:346 "ocr_text_of(&sel)"` | 树解析器永不匹配 OcrText,故 MCP 侧改为先看是不是 ocrText 再分发到 `find_by_text_ocr`,与 maestro 适配层同路 | — | 2026-07-22 |
| ④ | iOS runner 无 `/input-text`,Swift SDK `App.fill` 后半 404 | fixed | `at swift-bridge/Sources/SmixRunnerCore/InputTextRoute.swift:14 "public enum InputTextRoute"` | 路由在仓库源里存在并注册;此前抽验钉的是 `~/.local/share/smix/runner/` 派生副本,已改钉仓库路径 | — | 2026-07-22 |
| ⑤a | iOS `/tap` 只解 `selector.text`,Id/Label/Role/regex 选择器解不出来 | fixed | `at crates/smix-driver/src/lib.rs:1530 "fn require_runner_resolvable_selector"` | 三条路由(`/tap` `/double-tap` `/long-press`,实证同病)统一走新的 `RouteSelector`,wire 上认 text / id / label 三形态;`.text` 的「label 或 identifier」语义逐字节保留,新形态精确匹配。regex / role / modifier 仍主机侧解 —— 进 runner 就是一份契约两个实现。`04-actions.md` 那条教用户写 id 加 daemonProxy 的示例,今天起是真的 | — | 2026-07-22 |
| ⑤b | 上述 400 之后被 transport-retry 烧 5s | fixed | `at crates/smix-runner-client/src/lib.rs:839 "let retryable = e.is_connect()"` | `send_with_retry` 只在传输层错误(connect / request)时重试;HTTP 响应走 `return Ok(res)` 直接返回,4xx 不再叠 5 秒 | — | 2026-07-22 |
| ⑥ | iOS pressKey 方向键 404,键映射表仅 5 键 | fixed | `at swift-bridge/SmixRunnerUITests/SmixRunnerUITests.swift:1754 "arrowUp"` | 键表 9 键:return/delete/tab/space/escape + 四方向;home 走 XCUIDevice,lock/volume 明确报 unsupported 而非静默成功 | — | 2026-07-22 |
| ⑦ | `--retry` attribution 的 errorClass 映射与 adapter 退出码表不符 | fixed | `at crates/smix-cli/src/main.rs:2165 "PARSE_ERROR"` | 2=PARSE_ERROR / 3=EXPECTATION_FAILURE / 4=UNKNOWN_VERB / 5=FLOW_IO_ERROR / 6=RUNNER_UNREACHABLE,与 adapter 的 return 码逐码对齐;旧表把 parse 错归因成 timeout | — | 2026-07-22 |
| ⑧ | `smix run` 不读注册表 `runnerPort` | fixed | `at crates/smix-cli/src/main.rs:3460 "fn run_port"` | **C1 把这条记错了**:`smix run` 自 07-19 起就是 flag/env → `lookup_registered` → 22087,C1 只读了服务单点 verb 的 `runner_port_from_env` 就下结论、没跟到 `Cmd::Run` 分支。本段没修代码,补了 4 条断言(含「有 flag 时不读注册表」的惰性)并把行改真 | — | 2026-07-22 |
| ⑨a | `smix annotate` 帮助示例与实现不符 | fixed | `at crates/smix-cli/src/main.rs:2418 ""circle" =>"` | 帮助里的 `circle,at:100_100,color:red,radius:40` 实测越过 spec 解析(报错停在 decode input PNG) | — | 2026-07-22 |
| ⑨b | `smix authoring suggest` 帮助示例与实现不符 | fixed | `at crates/smix-cli/src/authoring.rs:106 "let candidates: [Option<&str>; 5]"` | **真跑之后答案是「跑不通」**:裸字符串形态只查 text/value/title,而实测 Settings 树里 label 33 个非空、text 与 title **各 0 个** —— 帮助印的 `suggest 'Sign In'` 在 iOS 上结构性地永远返回空。改为搜所有可读串后,`suggest 'General'` 在真机上返回 3 个候选。222KB 实测树已提交为 fixture,此后不需设备即可复查 | — | 2026-07-22 |
| ⑩ | `smix describe` 承诺字段恒空 | fixed | `at crates/smix-driver/src/lib.rs:1707 "pub fn front_app_of"` | `front_app` 从树根 identifier 取(类型改 Option —— None 不是空串,空串是「不知道」伪装成「知道」);`captured_at` 取抓树时刻墙钟;`summary` **收窄承诺**(字段文档自己写 caller-populated,无唯一来源)。连带修 runner 一行:根 identifier 从启动期常量改为每请求解析到的 bundle —— 平时对,只在切换 app 时错,而那正是唯一有人读它的时候 | — | 2026-07-22 |
| ⑪ | setPermissions iOS 无映射权限静默 Ok | fixed | `at crates/smix-sdk/src/ios_device.rs:236 "has no iOS mapping"` | `to_simctl()` 返回 None 的真实分支里 eprintln 报出,不再静默;跨平台 yaml 的 Android-only 权限仍不崩 iOS | — | 2026-07-22 |
| ⑫ | `swipe: {direction:}` maestro 形态缺口 | fixed | `at crates/smix-adapter-maestro/src/parser.rs:1211 "direction"` | `parse_swipe` 认 `direction`,四方向脱糖到中间行程 | — | 2026-07-22 |
| ⑬ | MCP:SMIX_UDID 要求不一致 + stop_app already-stopped 未容忍 + assert_visible 绕过 session guard | fixed | `at crates/smix-mcp/src/main.rs:516 "Already-stopped is a no-op success"` | 13 个 tool 描述统一声明 SMIX_UDID;stop_app 明确把 already-stopped 当成功;第三子项 07-19 已判误报并保持该判定 | — | 2026-07-22 |
| ⑭ | `exit_code_to_u8` 解析 Debug 字符串且失败映射为 0 | moot | `none "exit_code_to_u8" in crates/**/*.rs` | 该函数已不存在;`run_flow_code() -> u8` 直接返回码,描述所依赖的构造整个不在树里 | — | 2026-07-22 |
