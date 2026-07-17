# plan-hot — v2 到 C10：Kotlin SDK 改调 FFI 驱动面

## 目标 checkpoint

C10：Kotlin SDK(`dev.smix.sdk`)删掉 `HttpSmixSimRuntime.kt`(那套虚构 wire)，App 改经 FFI 驱动面驱动。与 C9(Swift)逐项对应，同样 6 处公开 API break。

**只做 Kotlin。** TS = C11。`route-conformance` 在 C10 出口**仍红**(TS 还引着虚构 route)，它的 rc=0 是 **C11** 的出口 —— 整个 SDK 手术的收口。

## 前置条件

```bash
git branch --show-current                                    # feature/v2.0
git status --short | grep -c .                                # 0（干净树）
test -e swift-bridge/Sources/SmixSDK/HttpSmixSimRuntime.swift && echo BAD || echo "C9 done"
python3 scripts/dev/route-conformance.py >/dev/null 2>&1; echo "route rc=$? (预期 1)"
```

## 已确证的起点（本次热化实测，非转述）

- **Kotlin 与 Swift 高度对称**：同样的文件集(App/Session/Smix/SimRuntime/HttpSmixSimRuntime)、同样的 `runtime.snapshotTree/synthesizeTap/sendString/pressKey/swipe/screenshot/…` 驱动模式。C9 的 Swift 手术是可复用蓝本。
- **同样的两个坑**(C9 已在 Swift 侧修，Kotlin 侧同型待修)：
  - `KeyName.ENTER` 无 FFI 家(`App.kt:139` = RETURN/DELETE/SPACE/TAB/ESCAPE/**ENTER**;Rust `KeyName` 无 Enter，`return` 才是回车)。且现有 `wireName`(`HttpSmixSimRuntime.kt:223-225`)发 PascalCase `"Return"` —— **旧的本就坏**。FFI 的 `parse_wire_enum` 收 camelCase(`return`/`arrowUp`)。
  - `App.systemPopups(): List<A11yNode>`(`App.kt:128`)是**虚构形状** —— FFI `system_popups()` 序列化 `Vec<SystemPopup>`(id/type/source/title/body/buttons)。C9 已建 Swift `SystemPopup`;Kotlin 需对应新建，字段与 `smix-runner-wire::SystemPopup` 对齐。
- **关键约束(与 C9 不同)**：**Kotlin host 单测加载不了 `libuniffi_smix.so`**(`SelectorResolver.kt:7` 实测注释；.so 是 Android-only)。所以 App 改调后**连构造都需要 `.so`**，现有 mock 测试无法直接适配。
- **既有 seam 模式可复用**：`SelectorResolver` / `LabelResolver` 是 `fun interface`(`SelectorResolver.kt:16,25`)，App 构造注入，默认实现包 UniFFI binding，测试注入 in-memory mock 避开 JNA 加载 .so。**驱动面照此建 seam**。

## 步骤（线性，无分叉）

### S1. Kotlin SDK 换 FFI 驱动面 + reshape App/Session/Smix.launchApp

**红（写测试）**

- 文件：`android-runner/sdk/src/test/kotlin/dev/smix/sdk/`
- 断言：删除靠 `MockSimRuntime` 驱动 wire 的 `*MockTest`(它们引用删除后的 `SmixSimRuntime` protocol，编译失败=红)。把保留的 shape/selector/ExpectationFailure 测试改到新构造形态。
- **驱动真覆盖在 C8 的 Rust wiremock，不在 Kotlin 侧**。Kotlin host 测试只证 FFI 之上的 App 逻辑(resolve→tapById 编排、failure 形状)——经 seam 注入 in-memory Driver/Session mock。**这不是"mock wire 自证"**(那测 HTTP 字节，是虚构 wire 出厂的病因)；这测的是 App 编排逻辑，wire 字节由 C8 覆盖。

**绿（实现）**

- **建驱动 seam**(照 `SelectorResolver` 模式)：`fun interface Driver { tree, openSession, listSessions }` + `Session`(acting 方法)，默认实现包 FFI `uniffi.smix.SmixDriver`/`SmixSession`，App 构造注入。**理由**：Kotlin host 测试无法加载 .so，seam 让 App 逻辑可测而不触 JNA;与既有架构一致(不发明新模式)。
- 删 `HttpSmixSimRuntime.kt`、`SimRuntime.kt`。
- `App.kt`：改经 seam 的 `Driver`(拿树)+ `Session`(动作)驱动。`tap`=resolve→`session.tapById(firstId)`;`fill`=resolve→tapById→`session.inputText`;`pressKey`=`session.pressKey(key.wireName)`;`swipe`=`session.swipeOnce(direction.wireName)`;`tapAtCoord`=`session.tapAtNormCoord`;`systemPopups`→`session.systemPopups()`解析`List<SystemPopup>`;`terminate`→`session.terminateApp`;`relaunch`→`session.relaunchApp`。
- **6 处 API break 移除**(同 C9)：`App.screenshot()` · `App.openUrl()` · `App.launchFresh()` 删除;`AppTarget.AppPath` case 删;绝对像素 tap→tapById;`Session` 的健康状态流(stateFlow/state)删。
- **KeyName.wireName 修**(同 C9)：映射到 FFI camelCase，`ENTER → "return"`(同一键)，其余小写首字母。SwipeDirection 同理(`UP → "up"`)。
- **新建 `SystemPopup.kt`**：字段对齐 `smix-runner-wire::SystemPopup`(id/type/source/title/body/buttons + button 的 id/label/role/dangerous/outcomeHint)。
- `Smix.kt`：`launchApp(target, runtime:)` 改签名，去 `runtime:` 参数，内部经 seam 默认实现构造(`SmixDriver(port)` → `openSession` → `launchApp`)。默认端口复用现有常量(grep 找 22087 的来源，不造第二份)。
- 关键点：App 同时握 Driver(tree/list)与 Session(act);`driver.tree()` 返回 JSON String 直喂 `resolveSelector`，省一次 re-encode(同 C9)。

**重构**

- 无新坏味则跳过。不"顺便"改 TS/Swift(§8.1)。

## Checkpoint C10 验收

```bash
# 1. Kotlin 虚构 wire 已删
test -e android-runner/sdk/src/main/kotlin/dev/smix/sdk/HttpSmixSimRuntime.kt && echo "STILL PRESENT" || echo "deleted"
# 2. Kotlin App 走 FFI 驱动 seam
grep -rcE "Driver|Session" android-runner/sdk/src/main/kotlin/dev/smix/sdk/App.kt 2>/dev/null | grep -v ":0$"
# 3. Kotlin SDK 不再含任何虚构驱动 route
grep -rcE "/sim/launch|/a11y/snapshot|/input/tap|/sim/screenshot|/sim/open-url" android-runner/sdk/src/main/kotlin/dev/smix/sdk/ 2>/dev/null | grep -v ":0$" || echo "Kotlin SDK: no fictional routes"
# 4. Kotlin SDK 测试:强制重跑,数 XML,failures=0
( cd android-runner && ./gradlew :sdk:test --rerun-tasks --console=plain >/tmp/c10kt.out 2>&1; echo "gradle rc=$?"
  find sdk -name "TEST-*.xml" | xargs grep -ho 'tests="[0-9]*"'    | grep -o '[0-9]*' | paste -sd+ - | bc
  find sdk -name "TEST-*.xml" | xargs grep -ho 'failures="[0-9]*"' | grep -o '[0-9]*' | paste -sd+ - | bc )
# 5. route-conformance 仍红(TS 未改 = C11;rc=0 是 C11 出口)
python3 scripts/dev/route-conformance.py >/dev/null 2>&1; echo "route rc=$? (预期 1)"
# 6. 无回归:Rust / clippy / hygiene / fence / bindings-fresh / swift(C9 不回退)
cargo test --workspace >/tmp/c10.out 2>&1; echo "cargo rc=$?"
grep -c "^test result: ok" /tmp/c10.out; grep -c "^test result: FAILED" /tmp/c10.out
cargo clippy --workspace --all-targets >/dev/null 2>&1; echo "clippy rc=$?"
python3 scripts/dev/hygiene-scan.py --noise-only >/dev/null 2>&1; echo "hygiene rc=$?"
bash scripts/dev/ffi-bindings-fresh.sh >/dev/null 2>&1; echo "bindings-fresh rc=$?"
( cd swift-bridge && swift test >/tmp/c10sw.out 2>&1; echo "swift rc=$?"; grep "Executed .* tests" /tmp/c10sw.out | tail -1 )
```

期望，逐条：

1. `deleted`(虚构 runtime 已删)。
2. App.kt 的 `Driver|Session` 引用计数 **≥1**。
3. `Kotlin SDK: no fictional routes`。
4. **`gradle rc=0`**;测试总数报出(会低于原基线 —— mock-wire 测试删除)、failures 求和 = **0**。**门是 0 failures**，不是数目不降(留 mock 凑数=gaming)。**BUILD SUCCESSFUL 不是证据**——`--rerun-tasks` 强制重跑 + 数 XML(它可在零执行时打 SUCCESSFUL)。
5. **`route rc=1`** —— 本段只改 Kotlin，TS 仍引虚构 route;rc=0 是 C11 出口。
6. `cargo rc=0`;`test result: ok` ≥ **132**(ai-tier 6 个 stub-CLI 测试预先失败、非本段回归，见 v2.md;若它们仍红，判据是"不新增失败")、`FAILED` 计数与基线一致;clippy/hygiene/fence/bindings-fresh `rc=0`;**swift `rc=0` 且 319/0**(C9 不回退)。

**仪器纪律**（本 cycle 反复吃亏）：
- **测退出码不接管道** —— `cmd | head; echo $?` 量的是 `head`。所有 rc `>/dev/null 2>&1; echo "rc=$?"` 单独取,或落 `/tmp` 再 grep。
- **不在编译未完成时读测试输出** —— 本 session 多次踩 `exit=101 / 22 buckets` 假读数(真值 132/0);落 `/tmp` 等命令整体结束再 grep。
- `./gradlew test` 的 `BUILD SUCCESSFUL` 可在零执行时打印 —— `--rerun-tasks` + 数 XML。
- **SDK 测试总数会降是预期的** —— 判据是 failures=0，不是数目不降。

**未被本 checkpoint 覆盖的**（写在明处，同 C3-C9 教训）：
1. **本段只做 Kotlin** —— TS(C11)一行不改，route-conformance 仍红。
2. **驱动真覆盖在 C8 的 Rust wiremock** —— Kotlin host 测试只经 seam 证 App 逻辑，不 mock wire。
3. **screenshot/openUrl/launchFresh 移除、走 CLI**(用户 2026-07-18 拍板);健康状态流丢失(FFI 不透 HTTP 头，无 FFI 家)。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2-c10-hot.md`
2. 调 sub-agent 生成新 `plan-hot.md`（到 C11：TS 删 13 虚构驱动 route + 依赖虚构 `/a11y/snapshot` 的 sense，保留 3-route resolver;`route-conformance.py` rc=0 是它的出口 —— 整个 SDK 手术的收口），见 CLAUDE.md §6

## 与冷计划不符之处（必须先读，不要隐瞒）

1. **Kotlin 需建驱动 seam，Swift 不需要** —— C9 的 Swift App 直接持 FFI `SmixDriver`/`SmixSession`(dylib 可在 host 加载)。Kotlin host 单测**加载不了 `.so`**(`SelectorResolver.kt:7`)，故 App 若直接持 FFI Object 则连构造都触 JNA 崩溃。解法照既有 `SelectorResolver` 模式建 `fun interface` seam，默认实现包 FFI，测试注入 in-memory mock。**这是 Kotlin 平台约束下的架构分歧，非省工** —— seam 让 App 逻辑可测，wire 正确性仍由 C8 的 Rust wiremock 独证。
2. **seam 的 in-memory mock ≠ "mock wire 自证"** —— 后者测 HTTP 字节(虚构 wire 出厂病因)，前者测 App 编排逻辑(resolve→tapById)。区别记在明处，防止被误读为回到病根。
