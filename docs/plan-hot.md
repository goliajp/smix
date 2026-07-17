# plan-hot — v2 到 C11：TS 删虚构 wire，SDK 手术收口

## 目标 checkpoint

C11：TS SDK(`@goliapkg/smix`)删掉 `HttpRunner.ts` 里那 13 条虚构驱动 route，保留被真正服务的 3-route resolver。**这是整个 SDK 手术的收口** —— `route-conformance.py` **rc=0**，三个 SDK 里再没有任何源码引用 runner 不服务的路由。

## 前置条件

```bash
git branch --show-current                                    # feature/v2.0
git status --short | grep -c .                                # 0（干净树）
test -e swift-bridge/Sources/SmixSDK/HttpSmixSimRuntime.swift && echo BAD || echo "C9 done"
test -e android-runner/sdk/src/main/kotlin/dev/smix/sdk/HttpSmixSimRuntime.kt && echo BAD || echo "C10 done"
python3 scripts/dev/route-conformance.py >/dev/null 2>&1; echo "route rc=$? (进入本段前预期 1，出口 0)"
```

## 已确证的起点（本次热化实测，非转述）

- **HttpRunner.ts 的路由 = 13 虚构 + 3 服务**(实测):删 `/sim/{launch,terminate,screenshot,open-url,launch-fresh,launch-from-path,system-popups}` + `/input/{tap,send-string,press-key,swipe,tap-normalized}` + `/a11y/snapshot`;留 `/select/resolve{,-count,-labels}`。
- **TS 与 Swift/Kotlin 的处境本质不同 —— TS 无 FFI 路径**(仓库零 napi/neon/wasm,C7/C8 实测)。故 TS **不是改调 FFI**,而是**删虚构驱动 + 保留 resolver + 动作级驱动 pending napi**(用户/C7-C8 决策已记)。
- **删驱动后 TS 连 sense 都塌**(实测):`App.resolveFirstOrThrow`(`App.ts:37`)靠 `runtime.snapshotTree()` 拿树喂 resolver,而 `snapshotTree` 走**虚构 `/a11y/snapshot`**。所以三条被服务的 resolver route 虽在,**却拿不到树可 resolve**。TS 的 App 动作级 + sense 级驱动**全部依赖虚构 route**。
- **诚实收场**:C11 后 TS = 类型(`A11yNode`/`Selector`/`Pattern`/`ExpectationFailure`/`Locator`)+ resolver 的**纯函数 seam**(给 `treeJson` 能 resolve,但树从哪来是 napi 的事)。**"从未工作过"的诚实收尾**,非能力倒退。
- **消费方**:`HttpSimRuntime` 被 `SimRuntime.ts`/`index.ts`/`Session.ts`/`App.ts`/`__tests__/HttpRunner.test.ts` 引用 —— 删它波及这些,逐个处理。
- **测试**:`npm test` = `vitest run`(`package.json:23`)。mock-wire 测试(`HttpRunner.test.ts` + App-driving mock 测试)删除;resolver/selector/类型/Locator 测试保留适配。

## 步骤（线性，无分叉）

### S1. TS 删虚构驱动 wire，保留 resolver seam，动作级驱动 pending napi

**红（写测试）**

- 文件：`npm/smix-rn/src/__tests__/`
- 断言：删 `HttpRunner.test.ts`(整文件 —— 它测的正是虚构 driving route 的 HTTP 字节,mock 自证,是虚构 wire 出厂的病因)。App-driving 的 mock 测试(经 `MockSimRuntime` 驱动动作)删除。**保留**:selector/pattern/类型 roundtrip、`Locator` 逻辑、resolver seam 的纯函数测试。
- **不写 mock-wire 断言**:resolver 的真覆盖是纯函数(给 treeJson + selectorJson → id list),不需要 runner;wire 层已随虚构 route 一起删。

**绿（实现）**

- 删 `HttpRunner.ts` 的 **13 条虚构驱动 route** 及其方法(`launch`/`terminate`/`snapshotTree`/`synthesizeTap`/`sendString`/`pressKey`/`swipe`/`screenshot`/`systemPopups`/`openUrl`/`launchFresh`/`launchFromPath`/`synthesizeTapAtNormalized`)。**保留** `resolveCount` + resolver 的三条 `/select/resolve{,-count,-labels}`(它们被服务)。
- **动作级 + sense 级驱动移除**:`SmixSimRuntime` interface(`SimRuntime.ts`)的驱动方法删;`App.ts` 的动作方法(`tap`/`fill`/`swipe`/`pressKey`/`tapAtCoord`/`screenshot`/`systemPopups`/`openUrl`/`launchFresh`/`terminate`/`relaunch`)与 sense(`snapshotTree`/`resolveFirstOrThrow` 靠虚构 snapshot)**移除或标注 `SmixNotImplementedError('pending napi')`**。**判据**:任何依赖虚构 route 的公开方法不能留一个"看起来能用实际 404"的表面(那正是本 cycle 一路在消灭的病)。
- **保留的 TS 表面**:类型(`A11yNode`/`Rect`/`Selector`/`Pattern`/`ExpectationFailure`/`A11yRole`)、`Selector` 构造/fluent、`Locator`(纯逻辑)、resolver seam(`resolveSelector`/`resolveCount`/`resolveLabels` 纯函数,给 treeJson)。
- **KNOWN DEFECT 注释块**(`HttpRunner.ts` 顶部,C6 加的)—— 缺陷现已修完(虚构 route 删净),**注释同步删**(否则它是下一条陈旧注释,本 cycle 已因陈旧注释翻车九次)。
- `index.ts`:去掉删除类型的 re-export;`Session.ts`:若整体依赖 HttpSimRuntime 驱动则删或降级为 resolver-only。
- 关键点:`route-conformance.py` 读 git 里所有源码的路由字面量。删净后它 rc=0 —— 这是本段(也是整个 SDK 手术)的机器可判出口。

**重构**

- 无新坏味则跳过。不"顺便"改 Swift/Kotlin/Rust(§8.1)。

## Checkpoint C11 验收

```bash
# 1. SDK 手术收口：route-conformance rc=0（三个 SDK 全部不再引用虚构 route）
python3 scripts/dev/route-conformance.py >/tmp/c11route.out 2>&1; echo "route rc=$?"
grep -E "clean —|no runner serves" /tmp/c11route.out | head -1
# 2. HttpRunner.ts 只余 resolver 三条服务 route
grep -oE "post\('(/[a-z0-9/-]+)'" npm/smix-rn/src/HttpRunner.ts 2>/dev/null | sort -u
# 3. 没有"看起来能用实际 404"的表面：删的驱动方法不再是可调公开 API
grep -cE "async (launch|screenshot|snapshotTree|synthesizeTap|sendString|openUrl|launchFresh)\(" npm/smix-rn/src/HttpRunner.ts
# 4. TS 测试：vitest 0 failed（读末尾汇总，不接管道吞 exit）
( cd npm/smix-rn && bun x vitest run >/tmp/c11ts.out 2>&1; echo "vitest rc=$?"; grep -E "Test Files|Tests " /tmp/c11ts.out | tail -2 )
# 5. KNOWN DEFECT 陈旧注释已删
grep -c "KNOWN DEFECT" npm/smix-rn/src/HttpRunner.ts
# 6. 无回归：Swift / Kotlin / Rust 不回退
( cd swift-bridge && swift test >/tmp/c11sw.out 2>&1; echo "swift rc=$?"; grep "Executed .* tests" /tmp/c11sw.out | tail -1 )
( cd android-runner && ./gradlew :sdk:test --rerun-tasks --console=plain >/dev/null 2>&1
  find sdk -name "TEST-*.xml" | xargs grep -ho 'failures="[0-9]*"' | grep -o '[0-9]*' | paste -sd+ - | bc )
cargo test --workspace >/tmp/c11.out 2>&1; echo "cargo rc=$?"
grep -c "^test result: ok" /tmp/c11.out; grep -c "^test result: FAILED" /tmp/c11.out
cargo clippy --workspace --all-targets >/dev/null 2>&1; echo "clippy rc=$?"
python3 scripts/dev/hygiene-scan.py --noise-only >/dev/null 2>&1; echo "hygiene rc=$?"
bash scripts/dev/ffi-bindings-fresh.sh >/dev/null 2>&1; echo "bindings-fresh rc=$?"
```

期望，逐条：

1. **`route rc=0`**,`/tmp/c11route.out` 含 `route-conformance: clean —`(不再有 `no runner serves`)。**这是 C11、也是整个 SDK 手术的出口。**
2. 只列出 `/select/resolve`、`/select/resolve-count`、`/select/resolve-labels`(三条服务 route)。
3. 计数 **0**(虚构驱动方法不再是可调公开 API)。
4. **`vitest rc=0`**,`0 failed`(测试总数会低于原基线 —— mock-wire 测试删除;门是 0 failed,不是数目不降)。
5. 计数 **0**(陈旧 KNOWN DEFECT 注释已删)。
6. **swift `319/0`、Kotlin failures=`0`、cargo `rc=0`**(`test result: ok` ≥ 132、`FAILED` 计数与基线一致 —— ai-tier 6 个 stub-CLI 预先失败非本段回归,见 v2.md);clippy/hygiene/bindings-fresh `rc=0`。

**仪器纪律**（本 cycle 反复吃亏）：
- **测退出码不接管道** —— `cmd | head; echo $?` 量 `head`。rc 单独 `>/dev/null 2>&1; echo "rc=$?"` 或落 `/tmp` 再 grep。
- **不在编译未完成时读测试输出** —— 本 session 多次踩 `exit=101 / 22 buckets` 假读数(真值 132/0);落 `/tmp` 等命令整体结束再读。
- `bun x vitest run` 读末尾 `Test Files`/`Tests` 汇总行,别中途截断。
- **SDK 测试总数会降是预期的** —— 判据 0 failed,不是数目不降。

**未被本 checkpoint 覆盖的**（写在明处，同 C3-C10 教训）：
1. **TS 动作级 + sense 级驱动本段后不存在** —— pending napi(独立 deliverable,§13 非省工)。C11 后 TS = 类型 + Selector + resolver 纯函数 seam。这是记录在案的欠账。
2. **napi 分发轴**是 v2 之后的独立工作(C7/C8/C9 已记),不在 SDK 手术三段(C9/C10/C11)内。
3. **route-conformance rc=0 只证"不再引用不存在的 route"**,不证真设备行为 —— 那属 ship gate。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2-c11-hot.md`
2. 调 sub-agent 生成新 `plan-hot.md`（到 C12：四个改名/合并 break —— #1 sessions 强制的另一半(去 Rust/CLI 隐式 no-session)· #3 `SMIX_*` 折进 config · #4 `Modifier(s)`+双 `open_url` 合并 · #5 `smix-recorder-ir`→`smix-authoring-ir` + `SimctlError` 改名），见 CLAUDE.md §6
3. **里程碑**:C11 收口后,四个已发布 SDK 里三个的虚构 wire 全清,唯一 wire client(Rust)经 FFI(Swift/Kotlin)暴露,TS 待 napi。这是 v2「一份 wire client」架构目标的达成点。

## 与冷计划不符之处（必须先读，不要隐瞒）

1. **TS 删驱动后连 sense 都塌** —— 冷计划把 TS 半段写成"删 13 route + 保留 3-route resolver",隐含 resolver 留下就还能用。实测 resolver 靠 `App.snapshotTree()`(虚构 `/a11y/snapshot`)拿树,**删驱动后 resolver 拿不到树可 resolve**。故 C11 后 TS 无 live sense,只余纯函数 resolver seam(给 treeJson 才动)。这是比冷计划字面更彻底的"从未工作过"收尾。
2. **TS 不建 FFI seam(与 Kotlin C10 不同)** —— 无 napi 路径,不像 Kotlin 有 `.so` 可包。TS 的动作级驱动直接移除、pending napi,不造 seam(造了也无默认实现可接)。
