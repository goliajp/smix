# plan-hot — v2.4 到 C4:清单清零

## 目标 checkpoint

**C4**:`docs/guide-executability.md` 里 **8 行全部 `runs`**,每一条都能在装回缺陷时重新变红。
`guide-executability` 摘要行读作 `8 claims (8 runs / 0 broken / 0 unjudged)`。

## 前置条件

```bash
git status --short
# 期望:空(C3 已提交)

cargo test -p smix-cli --bin smix guide_gate -- --nocapture 2>&1 | grep 'guide-executability:'
# 期望:8 claims (4 runs / 4 broken / 0 unjudged) · 69 yaml blocks judged

bash scripts/dev/preflight.sh
# 期望:最后一行 preflight: clean
```

---

## 四条的处置,已按 §12.2 逐条判过,执行期不得再议

判据统一是「**这是 core 能力缺位,还是页面写错了**」。两条各占一半。

### N3 —— 页面把两条路由的机制说反了。**改文档。**

`04-actions.md` §Default tap 现在写着:元素有 `accessibilityIdentifier` 时默认 tap 走
swift `/tap-by-id`,而 `/tap-by-id` 用 IOHID `_XCT_synthesizeEvent`;否则回退 XCUI
`element.tap()`。**三句话三处不对**,依据是两个 handler 自己的文档
(`SmixRunnerServer.swift:531` 与 `:539`):

| 页面说 | 实际 |
|---|---|
| 默认 tap 走 `/tap-by-id` | 默认走 `/tap-at-norm-coord`(`IosDriver::tap` 主机侧解树 → 归一化坐标) |
| `/tap-by-id` 是 IOHID synthesize | `/tap-by-id` 是 `XCUIElement.tap()`,即 XCTest 手势识别链 |
| Path A / Path B 二选一回退 | 没有这个回退。`/tap-by-id` 只由 `dispatch: xcui` 显式进入 |

`/tap-at-norm-coord` 是 `coordinate(withNormalizedOffset:).tap()` —— **Apple 原生事件链**,
它才是能触发 RN Pressable 的那条;`_XCT_synthesizeEvent` 属于 `dispatch: daemonProxy`。
页面顶部 §Action mental model 的那张图同错,一并改。

**没有能力缺位**:三条机制都在,只是页面把名字和用途对错了位。

### N5 —— 三个键名里两个有真名、一个没有对应物。**改文档。**

`pressKey` 的「Available keys」列了 `BACK` / `POWER` / `SCREEN_LOCK`,`KeyName` 三个都没有。
逐个问「core 缺这格能力吗」:

- **BACK** —— 不缺。返回导航是 **`- back`** 这个动词(`parser.rs:2499` → `App::go_back`)。
  `parse_key_name` 里那条注释写明了 `back` **故意**不做 `pressKey` 别名的理由:
  曾经的别名把每个 `- back` 变成静默退格并报成功。**按键与导航是两件事,不合并**,
  页面改成指向 `- back`
- **SCREEN_LOCK** —— 不缺,真名是 **`LOCK`**(`KeyName::Lock` → `XCUIDevice.perform(.lockButton)`)
- **POWER** —— 没有对应物,iOS 的公开 API 里也没有。**从列表里删**,不发明

另外页面这一节还漏了一件读者会撞上的事:`pressKey: VOLUME_UP` / `VOLUME_DOWN` 在
**iOS 模拟器上是 skip 不是执行**(Apple 的 XCUIDevice.Button 限制,`runtime.rs` 那段注释
写着 maestro 同样受限)。补一句。

### N6 —— 断言语言没有关系运算符。**补文法。**

`assertTrue: ${output.userCount > 0}` 里的 `>` 在词法上就落 `UnexpectedToken`;
`expr.rs:12` 的文法里 `eq` 已是最紧的比较层。

**这是能力缺位**:一门连大小比较都没有的断言语言确实弱,而页面把它当成有。
按 §12.2 补 core:在 `eq` 与 `unary` 之间插一层 `rel`:

```text
eq   = rel  (("==" | "!=") rel)*
rel  = unary (("<" | "<=" | ">" | ">=") unary)*
```

语义预先定死,不在执行期再议:
- 两边都是 `Number` → 数值比较
- 两边都是 `String` → 字典序(`str` 的 `Ord`)
- 其余混合类型 → **报错**,不做 JS 那种隐式转换。理由与该文件头部既有的取舍一致
  (「不支持的构造报 `UnsupportedPattern` 而不是静默 no-op」)——
  静默的类型转换正是这类表达式最容易骗人的地方
- `Null` 参与比较 → 报错

### N7 —— 想写正则,没有写法。**补解析。**

`03-selectors.md` 说「含 regex meta 字符即自动识别」,实际 `text_to_pattern` **只认 `|`**;
页面自举的 `^Help$` 与 `Row #[0-9]+` 都退化成字面量相等匹配。

**两件事,都要做**:

1. **不扩大自动识别**。把 `.` `?` `[` 也当 meta 会让 `Delete?` / `3.5` / `Row [1]`
   这类**普通标签**悄悄变成正则并匹配过宽 —— 比现在更糟,因为它不报错。
   页面那句话改成如实描述:`|` 触发;其余要显式写
2. **补显式写法(这才是能力缺位)**。`smix_selector::Pattern` 的 wire 形态
   `{regex, flags}` **早就存在**,但 yaml 解析器的 `text` 分支用 `.and_then(Value::as_str)`,
   映射形态直接掉出去 —— 于是**没有任何 yaml 写法能刻意造出一个 `Pattern::Regex`**。
   让 `text:` 接受已有的 tagged 形态:

   ```yaml
   - tapOn:
       text: { regex: "^Help$" }
   ```

   `flags` 沿用 `Pattern` 已有的 `default_regex_flags`(`"i"`),**不新造语义**

---

## 步骤(线性,4 个,一条 finding 一步)

### S1. N3 —— 让页面说的路由与轨迹一致

**红(写测试)**

- 文件:`crates/smix-cli/src/guide_gate.rs`
- `the_default_tap_still_misses_the_route_its_page_names` 翻正向,改名
  `the_default_tap_takes_the_route_its_page_names`:轨迹必须是 `Tap`(主机解析),
  **而页面必须这么写** —— 断言 04-actions §Default tap 的正文里出现
  `/tap-at-norm-coord` 且**不**出现「`/tap-by-id` 是默认」的说法
  (用页面里确实存在的字符串做判据,不用正则猜)
- 跑:红

**绿(实现)**

- 文件:`docs/ai-guide/04-actions.md` —— §Action mental model 与 §Default tap 按上表重写;
  三条路由各自的用途以 `SmixRunnerServer.swift` 的 handler 文档为准
- 跑:S1 转绿

### S2. N5 —— 让「可用键」列表与 `KeyName` 一致

**红(写测试)**

- 文件:`crates/smix-cli/src/guide_gate.rs`
- 新 probe `every_documented_key_name_parses`:从 04-actions 的「Available keys」那一行
  抽出全部键名,逐个喂 `smix_adapter_maestro` 的键名解析,全部必须成功
  (解析函数是 crate 私有 → 用一条 `pressKey: <KEY>` 的最小 flow 跑派生臂 1 的同一条路径)
- 跑:红,点名 `BACK` / `POWER` / `SCREEN_LOCK`

**绿(实现)**

- 文件:`docs/ai-guide/04-actions.md` / `02-yaml-reference.md` / `08-cookbook.md` ——
  三页里的 `pressKey: BACK` 改 `- back`;`POWER` 删;`SCREEN_LOCK` 改 `LOCK`;
  补 iOS 模拟器上 VOLUME_* 被 skip 的一句
- 从 `KNOWN_BROKEN` 删掉 N5 的两条(`04-actions` #13、`08-cookbook` #17)与
  `02-yaml-reference` #7 —— 它们现在应当能跑;**留着会让派生臂报「listed-as-broken 现在能跑了」**
- 跑:S2 转绿,派生臂 1 仍绿

### S3. N6 —— 补关系运算符

**红(写测试)**

- 文件:`crates/smix-adapter-maestro/src/expr.rs` 的 `#[cfg(test)] mod`
- 六条:`>` `<` `>=` `<=` 各一条数值真/假、一条字符串序、一条混合类型报错
- 跑:红

**绿(实现)**

- 文件:`crates/smix-adapter-maestro/src/expr.rs` —— 词法加四个 token,文法插 `rel` 层,
  求值按上面定死的语义;文件头 grammar 注释同步
- 从 `KNOWN_BROKEN` 删掉 `02-yaml-reference` #3
- 跑:S3 转绿

### S4. N7 —— 补显式正则写法,并把页面改成实话

**红(写测试)**

- 文件:`crates/smix-cli/src/guide_gate.rs`
- `the_documented_regex_examples_are_still_literals` 翻正向,改名
  `the_documented_regex_examples_are_patterns`:页面里印出来的每个正则示例,
  经解析后必须是 `Pattern::Regex`
- 文件:`crates/smix-adapter-maestro/tests/parser.rs` —— 加 tagged 形态解析的用例
- 跑:红

**绿(实现)**

- 文件:`crates/smix-adapter-maestro/src/parser.rs` —— `tapOn` / `assertVisible` 等
  走 `visible_to_selector` 的选择器位置,`text:` 接受 `{regex, flags}` 映射形态
  (**一处改动**:那些 verb 共用同一个 selector 读取路径,别分叉)
- 文件:`docs/ai-guide/03-selectors.md` —— 自动识别那句改成 `|`;两个示例改显式写法
- 跑:S4 转绿

---

## Checkpoint C4 验收

```bash
cargo test -p smix-cli --bin smix guide_gate -- --nocapture 2>&1 | grep -E 'guide-executability:|test result:'
grep -c '| broken |' docs/guide-executability.md
bash scripts/dev/preflight.sh
```

期望:

1. `guide-executability: 8 claims (8 runs / 0 broken / 0 unjudged) · … yaml blocks judged`;
   且 `test result: ok. … 0 failed`
2. 第二条输出 `0`
3. 第三条最后一行 `preflight: clean`

## 完成后动作

1. `mv docs/plan-hot.md docs/plan-history/v2.4-c4-hot.md`
2. v2.4 冷计划的出口验收在此成立 → 回 `docs/roadmap.md` 与 `docs/v2.md`,
   确认 v2 是否还有未闭合的段;若无,下一份热计划覆盖 v2.0.0 的发布前收口
   (`docs/scope-decisions-pending.md` 里三条待拍板仍未拍,那是**委托方的决定**,
   不是可以自己推进的工作 —— 收口计划要把它列为阻塞项而不是绕过它)
