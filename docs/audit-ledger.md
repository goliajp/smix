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
| ② | `webview_eval` Rust client 硬编码 `127.0.0.1:28080/eval`,与 Kotlin `/webview-eval`(28081 代理)路径端口双错 | present | `at crates/smix-runner-client/src/lib.rs:1330 "http://127.0.0.1:28080/eval"` | Android 半已不可达(`crates/smix-driver/src/android.rs:605` 改走 `webview_eval_via_runner` 代理);仍可达的是 iOS 直连 app 内 bridge 那条 —— 端口写死,换端口的 runner 上必失败。**是否设计如此待定,C3 收窄** | rust-client | 2026-07-22 |
| ③ | MCP `ocrText` 全路径恒失败,`assert_not_visible(ocrText)` 恒假绿 | fixed | `at crates/smix-mcp/src/main.rs:166 "ocr_text_of(&sel)"` | 树解析器永不匹配 OcrText,故 MCP 侧改为先看是不是 ocrText 再分发到 `find_by_text_ocr`,与 maestro 适配层同路 | — | 2026-07-22 |
| ④ | iOS runner 无 `/input-text`,Swift SDK `App.fill` 后半 404 | fixed | `at swift-bridge/Sources/SmixRunnerCore/InputTextRoute.swift:14 "public enum InputTextRoute"` | 路由在仓库源里存在并注册;此前抽验钉的是 `~/.local/share/smix/runner/` 派生副本,已改钉仓库路径 | — | 2026-07-22 |
| ⑤a | iOS `/tap` 只解 `selector.text`,Id/Label/Role/regex 选择器解不出来 | present | `at swift-bridge/Sources/SmixRunnerCore/TapRoute.swift:80 "selector.text not string"` | `tapOn: {id: …}` → SDK → `/tap` → 该 guard 直接 `DecodeError`。Swift 侧从未补过非 text 分支 | swift-runner | 2026-07-22 |
| ⑤b | 上述 400 之后被 transport-retry 烧 5s | fixed | `at crates/smix-runner-client/src/lib.rs:720 "let retryable = e.is_connect()"` | `send_with_retry` 只在传输层错误(connect / request)时重试;HTTP 响应走 `return Ok(res)` 直接返回,4xx 不再叠 5 秒 | — | 2026-07-22 |
| ⑥ | iOS pressKey 方向键 404,键映射表仅 5 键 | fixed | `at swift-bridge/SmixRunnerUITests/SmixRunnerUITests.swift:1720 "arrowUp"` | 键表 9 键:return/delete/tab/space/escape + 四方向;home 走 XCUIDevice,lock/volume 明确报 unsupported 而非静默成功 | — | 2026-07-22 |
| ⑦ | `--retry` attribution 的 errorClass 映射与 adapter 退出码表不符 | fixed | `at crates/smix-cli/src/main.rs:1636 "PARSE_ERROR"` | 2=PARSE_ERROR / 3=EXPECTATION_FAILURE / 4=UNKNOWN_VERB / 5=FLOW_IO_ERROR / 6=RUNNER_UNREACHABLE,与 adapter 的 return 码逐码对齐;旧表把 parse 错归因成 timeout | — | 2026-07-22 |
| ⑧ | `smix run` 不读注册表 `runnerPort` | present | `at crates/smix-cli/src/act.rs:35 "SMIX_RUNNER_PORT"` | `sim register --runner-port 22099` → `runner up` 用 22099(`main.rs` 的 `lookup_registered`),而 `smix run` 走 `runner_port_from_env`,只认 env 与默认 22087 → 连错端口 | cli | 2026-07-22 |
| ⑨a | `smix annotate` 帮助示例与实现不符 | fixed | `at crates/smix-cli/src/main.rs:1832 ""circle" =>"` | 帮助里的 `circle,at:100_100,color:red,radius:40` 实测越过 spec 解析(报错停在 decode input PNG) | — | 2026-07-22 |
| ⑨b | `smix authoring suggest` 帮助示例与实现不符 | present | `at crates/smix-cli/src/main.rs:483 "Suggest {"` | verb 存在(clap `Suggest` 变体),帮助里的 `smix authoring suggest 'id: qa-*'` 需 live runner 才能验示例是否原样跑通。**设备复验留 C3**;未跑就不记 fixed | cli+docs | 2026-07-22 |
| ⑩ | `smix describe` 承诺字段恒空 | present | `at crates/smix-driver/src/lib.rs:137 "front_app: String::new()"` | `smix describe` → `driver.describe()`:只填 `elements`,`front_app` / `summary` / `captured_at` 恒空零,而 CLI 帮助承诺 "title / interactive elements / status bar" | driver+swift-runner | 2026-07-22 |
| ⑪ | setPermissions iOS 无映射权限静默 Ok | fixed | `at crates/smix-sdk/src/ios_device.rs:194 "has no iOS mapping"` | `to_simctl()` 返回 None 的真实分支里 eprintln 报出,不再静默;跨平台 yaml 的 Android-only 权限仍不崩 iOS | — | 2026-07-22 |
| ⑫ | `swipe: {direction:}` maestro 形态缺口 | fixed | `at crates/smix-adapter-maestro/src/parser.rs:1146 "direction"` | `parse_swipe` 认 `direction`,四方向脱糖到中间行程 | — | 2026-07-22 |
| ⑬ | MCP:SMIX_UDID 要求不一致 + stop_app already-stopped 未容忍 + assert_visible 绕过 session guard | fixed | `at crates/smix-mcp/src/main.rs:307 "Already-stopped is a no-op success"` | 13 个 tool 描述统一声明 SMIX_UDID;stop_app 明确把 already-stopped 当成功;第三子项 07-19 已判误报并保持该判定 | — | 2026-07-22 |
| ⑭ | `exit_code_to_u8` 解析 Debug 字符串且失败映射为 0 | moot | `none "exit_code_to_u8" in crates/**/*.rs` | 该函数已不存在;`run_flow_code() -> u8` 直接返回码,描述所依赖的构造整个不在树里 | — | 2026-07-22 |
