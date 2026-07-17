# plan-hot — v2 到 C13：删死类型 `Modifier`（单数），#4 破坏性变更收尾

## 目标 checkpoint

C13：破坏性变更 #4 的两半都落定 —— **删掉从未接进 `Selector` 的死公开类型 `Modifier`（单数）**（Swift `public enum` + Kotlin `sealed interface` + 它们各自的纯构造测试块），**「双 open_url」记为已被 SDK 手术（C9/C10/C11）消解、无剩余工作**。通过后 SDK 的选择器模型只剩 `Modifiers`（复数，扁平，与 Rust wire 契约一致）这一种。

## 前置条件

```bash
git branch --show-current                                    # feature/v2.0
git status --short | grep -c .                                # 0（干净树）
# C12 两个改名已落
git grep -c "SimctlError" -- '*.rs' | wc -l                  # 0
test -d crates/smix-authoring-ir && echo "C12 done"
```

## 已确证的起点（本次热化实测，非转述）

- **`Modifier`（单数）是死类型**：Swift `Modifier.swift:17` `public enum Modifier`（9 case）+ Kotlin `Modifier.kt:10` `sealed interface Modifier`（同 9 case），但 **`Selector` 从不消费它** —— Kotlin `Selector.kt:45-57` 全部用 `Modifiers`（复数，扁平）+ `IndexModifiers`。`Modifier`（单数）唯一的"消费者"是各 SDK 的 `MvpApiShapeTest` 里一段"证明它 9 case 可构造"的纯构造断言（不测任何行为）。Rust `smix-selector` 无单数 `Modifier`（`:220` 只有 `Modifiers`）;TS 无。
- **它仍是公开 API**（Swift public / Kotlin sealed 默认 public），故删它是 v2 break —— 但删的是一个**从未接进选择器模型的僵尸类型**，非能力倒退。迁移：无 SDK 用户能用它做过任何事（构造出来也没有 Selector 收它）。
- **「双 open_url」在当前代码里已不存在**（实测）：Rust 侧全部 `open_url`、**无 `open_link` 定义**（`git grep "fn open_link|func openLink"` 空）;yaml 层 `openLink`（maestro）/ `openUrl`（smix）双拼写是 VERB_TABLE 每个 verb 的正常 alias/native 双列设计，不是缺陷（C12 已测两个都 parse OK）;SDK 侧曾有的 `openUrl` 方法已在 C9/C10 作为宿主侧 break 删除（TS 只剩抛错 stub）。**dossier 的「双 open_url」指的是 SDK 手术前 SDK 侧的两个开链接路径，已随 C9/C10/C11 消解。** 无剩余工作,记账即可。
- **规模**：删 2 个定义文件 + 2 个测试的 Modifier 构造块 + 无 index re-export（Kotlin 无 index;Swift module-level;TS 无该类型）。

## 步骤（线性，无分叉）

### S1. 删死类型 `Modifier`（单数）+ 其纯构造测试块

**红（写测试）**

- 无需先写红：删除后编译即验（删定义 + 删唯一引用它的测试块 = 编译面自洽）。若删测试块后 `MvpApiShapeTest` 的其余断言仍需 `Modifiers`（复数）则保留那些 —— 只删 `Modifier`（单数）相关的 `List<Modifier>` / `[Modifier]` 构造断言。
- **判据**：删后 `swift test` / `gradle :sdk:test` 编译通过且 0 failures;`Modifier`（单数）在 SDK 源码里清零（`Modifiers` 复数不受影响）。

**绿（实现）**

- 删 `swift-bridge/Sources/SmixSDK/Modifier.swift`（整文件 —— 只含单数 enum）。
- 删 `android-runner/sdk/src/main/kotlin/dev/smix/sdk/Modifier.kt`（整文件 —— 只含单数 sealed interface）。
- 删 `MvpApiShapeTest.kt` / `MvpApiShapeTests.swift` 里构造 `Modifier`（单数）9 case 的断言块（"Modifier cases" MARK 段）—— 那些只证"可构造"、不测行为,且构造的类型即将不存在。
- **不碰** `Modifiers`（复数）/ `IndexModifiers` / `AnchorBox` —— 它们是 `Selector` 真正使用的扁平模型,与 Rust wire 一致,是保留的单一模型。
- 关键点：删的是**从未接进 Selector 的僵尸公开类型**。SDK 的选择器建构（`Selector.id().below(...)` 等 fluent）全走 `Modifiers`（复数）,不受影响。

**重构**

- 若 `Modifier.swift` / `Modifier.kt` 顶部注释被别处 doc 引用则一并清（本 cycle 已多次因陈旧注释翻车）。不"顺便"改 TS/Rust（§8.1）。

## Checkpoint C13 验收

```bash
# 1. 死类型清零（Modifiers 复数不受影响）
grep -rln "enum Modifier\b\|sealed interface Modifier\b" swift-bridge/Sources/SmixSDK/ android-runner/sdk/src/main/ 2>/dev/null | wc -l   # 期望 0
test -e swift-bridge/Sources/SmixSDK/Modifier.swift && echo BAD || echo "swift Modifier deleted"
test -e android-runner/sdk/src/main/kotlin/dev/smix/sdk/Modifier.kt && echo BAD || echo "kotlin Modifier deleted"
# 2. Modifiers（复数，保留的单一模型）仍在
grep -c "struct Modifiers\|data class Modifiers" swift-bridge/Sources/SmixSDK/Modifiers.swift android-runner/sdk/src/main/kotlin/dev/smix/sdk/Modifiers.kt
# 3. Swift SDK 测试 0 failures（读 XCTest Executed 行）
( cd swift-bridge && swift test >/tmp/c13sw.out 2>&1; echo "swift rc=$?"; grep "Executed .* tests" /tmp/c13sw.out | tail -1 )
# 4. Kotlin SDK 测试 0 failures（--rerun-tasks + 数 XML）
( cd android-runner && ./gradlew :sdk:test --rerun-tasks --console=plain >/dev/null 2>&1
  find sdk -name "TEST-*.xml" | xargs grep -ho 'failures="[0-9]*"' | grep -o '[0-9]*' | paste -sd+ - | bc )
# 5. 无回归：Rust / clippy / hygiene / bindings-fresh / route（rc=0 不回退）
cargo test --workspace >/tmp/c13.out 2>&1; echo "cargo rc=$?"
grep -c "^test result: ok" /tmp/c13.out; grep -c "^test result: FAILED" /tmp/c13.out
cargo clippy --workspace --all-targets >/dev/null 2>&1; echo "clippy rc=$?"
python3 scripts/dev/hygiene-scan.py --noise-only >/dev/null 2>&1; echo "hygiene rc=$?"
bash scripts/dev/ffi-bindings-fresh.sh >/dev/null 2>&1; echo "bindings-fresh rc=$?"
python3 scripts/dev/route-conformance.py >/dev/null 2>&1; echo "route rc=$?"
```

期望，逐条：

1. 计数 **0** + 两个 `Modifier deleted`（单数死类型清零）。
2. `Modifiers`（复数）计数 ≥2（保留的单一选择器模型仍在）。
3. **swift `319/0`**（删纯构造断言,总数可能微降,门是 0 failures）。
4. Kotlin failures 求和 **0**。
5. cargo `rc=0`、`ok` ≥ 132、`FAILED` 与基线一致（ai-tier 6 stub-CLI 偶发,非本段）;clippy/hygiene/bindings-fresh `rc=0`;**route `rc=0`**（SDK 手术收口不回退）。

**仪器纪律**（本 cycle 反复吃亏）：
- **测退出码不接管道** —— `cmd | head; echo $?` 量 `head`。rc 单独 `>/dev/null 2>&1; echo "rc=$?"` 或落 `/tmp`。
- `swift test` 读 XCTest `Executed N` 行,不读 swift-testing `0 tests` 行。
- **不在编译未完成时读测试输出**（本 session 多次踩 `exit=101 / 22 buckets` 假读数,真值 132/0）。
- `./gradlew test` 的 `BUILD SUCCESSFUL` 可零执行,`--rerun-tasks` + 数 XML。

**未被本 checkpoint 覆盖的**（写在明处）：
1. **「双 open_url」无剩余工作** —— 已被 C9/C10/C11 的 SDK 手术消解,本段仅记账,不动代码。
2. cargo-semver-checks（证删公开类型是 semver break）是 C16 ship gate,本机未装。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2-c13-hot.md`
2. 调 sub-agent 生成新 `plan-hot.md`（到 C14：#1 sessions 强制 + #3 `SMIX_*`→config —— 行为收紧 + config 工程,含 `.smix/config.yaml` 与 `.smix/config.json` 统一决策），见 CLAUDE.md §6

## 与冷计划不符之处（必须先读，不要隐瞒）

1. **#4 的「合并」实为「删死类型」** —— 冷计划/dossier 写「`Modifier(s)` 合并单模型」,隐含两个活类型合二为一。实测 `Modifier`（单数）从未接进 `Selector`,是僵尸;真正的选择器模型一直是 `Modifiers`（复数,扁平,与 Rust wire 一致）。所以「合并到单模型」= 删掉那个从未参与的僵尸,而非融合两个活类型。
2. **「双 open_url」已不存在** —— C12 记的「留 C13 核清真义」已核：Rust 侧无 `open_link` 定义、只一个 `open_url`;SDK 侧的 `openUrl` 已在 C9/C10 删。dossier 的「双」指 SDK 手术前的两条路径,已消解。**本段对 open_url 零改动,只记账。** 上个规划 agent 的「openUrl 不可解析」前提是错的(C12 已记),此处彻底澄清。
