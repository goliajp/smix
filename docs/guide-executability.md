# guide-executability — 指南里印出来的每条声明,现在跑起来是什么样

这张表的每一行是 `docs/ai-guide/` 某一页做出的一条**可执行声明**,以及「它今天真的成立吗」。
状态不是人写上去的判断,是 `crates/smix-cli/src/guide_gate.rs` 每次运行重新求值的结果 ——
`probe` 列点名的那个测试就是求值器。

**为什么单独一张表,不并进现有两张。**
`docs/audit-ledger.md` 的行集锚在 `docs/v2.md`「待办(按严重度,未修)」那一行的圈号,
是 07-19 那次审计的 14 项;`docs/scope-evidence.md` 的行集锚在 `v2.md` in-scope 的编号项,
是「承诺 vs 实现」。本表的行集锚在**指南示例本身**。往任一张旧表里加行会让它的覆盖检查
失去锚(`v2.md` 决策日志 2026-07-22 [v2.3-C3] 末条已就此写死)。

`ledger` 列是与旧表的咬合点:取 `—`,或一个在 `docs/audit-ledger.md` 的 `#` 列里真实
出现过的圈号。闸门校验这一点,所以 `—` 是「这条不属于那次审计」的机器可查证据,
而不是一句没人核过的断言。

## 表

| id | 出处 | 声明 | status | probe | 依据(它会走的代码路径) | 层 | ledger | 复核 |
|---|---|---|---|---|---|---|---|---|
| N1 | `05-cli.md` §Environment-variable precedence | 端口取值走「flag → env → registry → default」四级 | runs | `every_runner_dialling_command_can_reach_the_registry` | 16 个会拨 runner 的子命令各加 `--device`,端口经 `main.rs runner_dial_port` 走 `run_port(flag.or(env), || device→registry.runner_port)`;`act.rs runner_port_from_env_opt` 返 `Option` 而非 `u16` —— 旧版把默认值填在链条中间,注册表那一级永远轮不到。默认值现在只有 `act::DEFAULT_RUNNER_PORT` 一处 | — | — | 2026-07-22 |
| N2 | `08-cookbook.md` `apps.yaml` 示例 | Android 启动 Activity 可由 `activity:` 配置 | runs | `a_configured_launch_activity_reaches_the_device` | `apps_config.rs resolve_app_into_flow` 把解析出的 activity 写进 `Flow::launch_activity`,`runtime.rs` 播给 `LaunchAppOptions::launch_activity`,`sdk/lib.rs launch_app_with_options` 传进 `DeviceControl::launch_with_args(.., activity)`;`android_device.rs` 用它,没有则 `entry_point()` 问 `cmd package resolve-activity`。runner 侧 `RunnerTest.entryPoint()` 走 `packageManager.getLaunchIntentForPackage`,`RunnerWire.foregroundCommand(bundleId, activity)` 只在两者都答不出时才落回 `.MainActivity` | — | — | 2026-07-22 |
| N3 | `04-actions.md` §Default tap | 元素带 `accessibilityIdentifier` 时,默认 tap 走 swift `/tap-by-id`(IOHID) | broken | `the_default_tap_still_misses_the_route_its_page_names` | 轨迹是 `Tap(Id)`:`crates/smix-driver/src/lib.rs:368 IosDriver::tap` 取树、主机侧解析、`tap_at_norm_coord`。`/tap-by-id` 只有两条入口 —— `dispatch: xcui`,以及 runtime 的 `v2-modal-*` / `v2-tab-*` 白名单 | docs | — | 2026-07-22 |
| N5 | `02-yaml-reference.md` / `04-actions.md` §Press hardware key / `08-cookbook.md` | `pressKey` 可用键含 `BACK` / `POWER` / `SCREEN_LOCK` | broken | `every_yaml_example_reaches_a_route` | `crates/smix-input/src/lib.rs:68 KeyName` 十三个变体里没有这三个;`runtime.rs:3427 parse_key_name` 的表里也没有,且 `back` 是**故意**不做 Delete 别名的(同处注释:曾经的别名把每个 `- back` 变成静默退格并报成功) | parser+docs | — | 2026-07-22 |
| N6 | `02-yaml-reference.md` `assertTrue` 示例 | `${output.userCount > 0}` 这类比较可用 | broken | `every_yaml_example_reaches_a_route` | `crates/smix-adapter-maestro/src/expr.rs:12` 的文法里 `eq` 已是最紧的比较层,只有 `==` / `!=`;关系运算符一个都没有,`>` 在词法上就落到 `UnexpectedToken` | parser+docs | — | 2026-07-22 |
| N7 | `03-selectors.md` §Text (literal or regex) | 字符串含 regex meta 字符即自动识别为模式(页面自举 `^Help$` / `Row #[0-9]+`) | broken | `the_documented_regex_examples_are_still_literals` | `crates/smix-adapter-maestro/src/parser.rs:453 text_to_pattern` **只在字符串含 `\|` 时**返 `Pattern::Regex`,其余一律 `Pattern::Text`;而 `smix-selector/src/lib.rs:539` 对 `CompiledPattern::Text` 走 `eq_ignore_ascii_case` 严格相等 —— 页面举的两个例子都退化成「找 label 恰好等于 `^Help$` 的元素」,不报错,只是永远找不到 | parser+docs | — | 2026-07-22 |
| P1 | `04-actions.md` §Tap with explicit dispatch | `tapOn: {id, dispatch: daemonProxy}` 可用 | runs | `the_daemon_proxy_id_example_is_admissible` | 轨迹是 `TapWithMode(Id, DaemonProxySynthesize)` → `POST /tap`;选择器再过一遍 `smix_driver::require_runner_resolvable_selector`,由 `lib.rs:1048` 的 `Selector::Id \| Selector::Label` 一臂放行 | — | ⑤a | 2026-07-22 |
| P2 | `smix authoring suggest --help` | 裸字符串(如 `'Sign In'`)是可用的检索形态 | runs | `the_bare_string_form_matches_a_real_tree` | `crates/smix-cli/src/authoring.rs:106` 的 `candidates: [Option<&str>; 5]` 让裸形态搜 label / text / value / title / identifier。对实测树 `tests/fixtures/live-tree-preferences-2026-07-22.json`(33 个非空 label,0 个 text、0 个 title)用 `General` 命中;同串走 `text:` 分支返回 0,后半句是红向注入 | — | ⑨b | 2026-07-22 |

## 状态词汇

- `runs` —— 闸门跑过它,走通了。`层` 必须是 `—`(没有要修的东西)
- `broken` —— 闸门跑过它,不成立。`层` 写清修它要动哪几层
- `unjudged` —— 闸门判不了。`依据` 必须写明要人做什么、哪个 checkpoint 结账;`probe` 写 `—`

## 这张表看不见什么

- **散文**。派生臂自动判「跑不跑得动」;「本页说它会走 X 路由」这类断言必须**手写 probe**。
  probe 名册是手维护的 —— 这是本表相对 `audit-ledger-scan`(行集由 `v2.md` 导出)更弱的一处
- **设备层**。元素是否真在屏上、IOHID 是否真触发 `onTap`、被测应用的 launcher activity
  是否恰好叫 `.MainActivity`。闸门判的是「会走哪条路径、那条路径接不接受」,不是「在某台机器上跑通了」
- **`KNOWN_BROKEN` 里 `NeedsDevice` 的那几条**。它们在 `guide_gate.rs` 里列着并计数,
  但语料里那几块没人判 —— 列出来是为了这张表的覆盖率不至于悄悄虚高

## 闸门怎么对着这张表检查

`crates/smix-cli/src/guide_gate.rs` 的 `the_list_and_the_probes_agree`:

- 表格必须 9 格。格数不对是**错误**不是跳过 —— 跳过等于让这一行从此不被任何检查看见
  (`audit-ledger.md` 曾因单元格里一个未转义的 `\|` 把整行拆成 11 格,悄悄漏掉一行)
- `runs` / `broken` 行的 `probe` 必须是闸门里真实存在的测试名;写了不存在的名字 = 红
- 每个 `N*` / `P*` probe 必须在表里有行;有 probe 没行 = 红
- `status` / `层` / `ledger` / `复核` 的取值与格式受校验;`ledger` 非 `—` 时该圈号必须在
  `docs/audit-ledger.md` 里出现过
- **不对称引用**:`broken` 行钉的是**缺陷代码**(修掉 → 引用失配 → 红 → 逼你改状态);
  `runs` 行钉的是**修复代码**(revert → 同样红)。与 `audit-ledger-scan` 同一设计
