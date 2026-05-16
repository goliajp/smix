# 设计文档

## 项目定位

iOS Simulator 自动化控制工具，面向 **TDD 开发** + **E2E 测试** 两个场景。

**只做模拟器，不做真机。**

核心差异化：**AI-native**——围绕 "Claude / Claude Code 这类 AI coding tools 在写什么样的东西时质量最好" 来设计 API、错误信息、工具链。

## 为什么不做真机

- 真机自动化的复杂度集中在 RSD tunnel / DDI / 签名 / Developer Mode，工程量占整体 40-50%
- TDD/E2E 场景下模拟器够用且更快
- 模拟器场景下可以激进使用 host 侧能力（CoreSimulator IPC、私有 HID 注入），延迟可压到 ms 级
- 砍掉真机后，工具链不必碰 pymobiledevice3 / go-ios / devicectl 真机分支

## 调研结论（两轮）

### 同类方案的真实底层

- **Maestro**：v1.18 起从 idb 路线切到 XCUITest + 自建 HTTP server (FlyingFox)。idb 的私有 framework 在 iOS 大版本升级时反复 break，团队用脚投票放弃。Apache-2.0。架构值得参考。
- **Appium + WDA**：W3C 协议 + XCUITest，行业基线，慢但稳。
- **idb / FBSimulatorControl**：Meta 系，大量私有 API，iOS 17+ 维护跟不上，事实弃疗。
- **Detox / EarlGrey**：gray-box，必须 link 进 App，不适合"控任意 app"场景。
- **mobile-mcp / ios-simulator-mcp**：MCP 包装层，前者用 WDA，后者用 idb（继承 idb 所有坑）。

### iOS 26 / Xcode 26 关键变化

- **`XCUIAutomation` 从 XCTest 拆成独立 framework**——Apple 正名 UI 自动化路径，公开 API 稳定性提升
- **模拟器 HID wire format 改了**：`IndigoHIDMessageForMouseNSEvent` 5 参数 → 9 参数，digitizer target 必须 `0x32`。idb / AXe / 老 Maestro 都因此在 iOS 26 模拟器上失效
- **`FoundationModels` 公开 framework**：device 上跑 Apple 小 LLM，免费 + 离线 + 隐私
- **`VisualIntelligence`**：是 App Intents 接入入口，**不是读屏 API**（产品宣传常见误区）
- **Vision framework**：`RecognizeDocumentsRequest` + 表格识别，OCR 兜底用
- **`xcrun devicectl` 增强但仍不能替代真机自动化全流程**——与本项目无关（不做真机）
- **`XCTHIDEventGenerator` 截至 Xcode 26.5 仍是私有**，无公开替代

### 模拟器场景下的最优路径

| 路径 | 延迟 | 合规度 | 选择 |
|---|---|---|---|
| 私有 HID 注入（iOS 26 = 9-arg Indigo） | ms 级（跳过 testmanagerd） | 私有 API，**模拟器场景可接受** | ✅ 主路径 |
| XCUITest `tap()` / `typeText()` | 数十 ms+ | 完全公开 | ✅ fallback |
| AccessibilityPlatformTranslation（host 直读 sim AX server） | <50ms | 私有 | ✅ AX 读取主路径 |
| `XCUIElementSnapshot` | 50-100ms | 公开 | ✅ AX 读取 fallback |
| Vision OCR + VLM | 200ms+ | 公开 | ✅ 视觉兜底 |

## 兼容矩阵

只支持 iOS 17+，重点是 iOS 26+。

| 维度 | iOS 17 | iOS 18 | iOS 26+ |
|---|---|---|---|
| HID 私有路径 | 5-arg Indigo | 5-arg Indigo | **9-arg Indigo, target 0x32** |
| AX 读取 | `XCUIElementSnapshot` 主 | 同 | + `AccessibilityPlatformTranslation` fast-path |
| Foundation Models | 无 | 无 | 可用 |
| 录制回放参考 | 自家 | 自家 | + 寄生 Xcode 26 Automation Explorer |

所有私有路径必须 `#available` 分支 + abstraction 包住，破时主路径（XCUITest）不受影响。

## 架构

```
┌──────────────────────────────────────────────────────┐
│  开发者 / AI 入口                                     │
│  ├─ CLI:  mytool run | record | repl | watch         │
│  ├─ SDK:  TypeScript test API（Playwright 风格）      │
│  └─ MCP server: AI agent 一等公民接口                 │
└────────────────┬─────────────────────────────────────┘
                 │
┌────────────────▼─────────────────────────────────────┐
│  Orchestrator (TS)                                   │
│  - DSL parser / runner                               │
│  - Selector resolver                                 │
│  - Watch mode                                        │
│  - 录制回放                                          │
│  - 失败时生成 AI-readable 错误                       │
└──────┬───────────────────────────────────┬──────────┘
       │                                   │
┌──────▼─────────────────┐    ┌────────────▼─────────┐
│ Sim Control (Swift/    │    │ Vision Fallback      │
│ macOS native bridge)   │    │  - Vision OCR        │
│  - CoreSimulator boot  │    │  - 截图 hash 对比    │
│  - simctl wrapper      │    │  - Foundation Models │
│  - 多 sim 并发         │    │    （iOS 26 可选）   │
└──────┬─────────────────┘    └──────────────────────┘
       │
┌──────▼───────────────────────────────────────────────┐
│ Input + AX Layer (Swift bridge, runs host-side)      │
│  - iOS 26: 9-arg Indigo HID + IOHIDEvent 树          │
│  - iOS 17/18: 5-arg Indigo                           │
│  - Fallback: XCUITest tap/typeText                   │
│  - AX: AccessibilityPlatformTranslation 主           │
│  - AX fallback: XCUIElementSnapshot via runner       │
└──────────────────────────────────────────────────────┘
```

## API 设计原则（AI-friendly）

### 1. 用 Playwright TS 风格，不用自创 YAML

- AI 训练语料里 Playwright 是 web 测试金标准，搬到 iOS 直接享受红利
- TS 类型 → AI 写时被类型系统约束 → 错误少
- 自创 YAML schema AI 没见过 → 必胡编字段名

### 2. Selector 必须语义化

```typescript
type Selector =
  | { text: string | RegExp }
  | { id: string }
  | { label: string }
  | { role: Role; name?: string }
  & {
    near?: Selector
    below?: Selector
    above?: Selector
    leftOf?: Selector
    rightOf?: Selector
    inside?: Selector
    nth?: number
    first?: boolean
    last?: boolean
  }
```

- **`role` 用 Playwright ARIA role**（`button` / `link` / `textField` ...）做表面，内部翻译到 `XCUIElement.ElementType`。翻译是工程活，换语义红利。
- **禁掉 xpath / 坐标 selector**——AI 写这两种必飘。

### 3. 不提供裸 sleep

AI 一旦能 sleep 就会乱 sleep。强制用 `waitFor(selector)` 表达意图。

### 4. 错误信息直接是 AI 修复 prompt

```typescript
type ExpectationFailure = {
  ok: false
  code: 'ELEMENT_NOT_FOUND' | 'NOT_VISIBLE' | 'TIMEOUT' | ...
  message: string                    // "Element { text: 'Sgin in' } not found"
  suggestions: string[]              // ["Did you mean 'Sign in'?", ...]
  visibleElements: ElementSummary[]
  screenshot: string                 // base64
  hint?: string
}
```

不是给人看的 stack trace，是 feed 回 AI 就能让它知道下一步改什么的修复 prompt。

### 5. 自研 expect

借鉴 Vitest/Jest 的 fluent API 形状，但他们的问题——错误格式给人不给 AI、snapshot 不适合屏幕、async 堆栈丢失——我们解决不了，必须自研。

设计要点：
- matcher 抛 `ExpectationFailure` 结构化对象，不是 `AssertionError`
- 失败时 matcher 内部 format 完整 AI prompt（含 visible elements + screenshot）
- 视觉 snapshot 直接做图像 diff，不是文本 diff

### 6. MCP server 是一等公民

- 所有交互 tool 默认返回 `{ ok, screen: ScreenDescription }`，省 AI 一次 round-trip
- `screen_describe` 是 AI 的眼睛——便宜 + 高信息密度
- 提供 `explain_screen(question)` 利用 host VLM 回答自然语言问题（AX 拿不到时兜底）

### 7. 录制即 few-shot 生成器

录制时同时记录：(代码, a11y trace, 截图序列)。这套 trace 是后续 AI 生成新测试时的 few-shot 上下文。

## DSL surface（v0）

### Actions（约 25 个）

- **App 生命周期**：launch / terminate / background / foreground / install / uninstall
- **交互**：tap / doubleTap / longPress / fill / clear / swipe / scroll / scrollTo
- **键盘**：pressKey / hideKeyboard
- **系统**：openUrl / pasteboard.set/get / permissions.grant / appearance / locale / statusBar.override / network
- **等待**：waitFor（不提供裸 sleep）
- **捕获**：screenshot / tree
- **断言**：expect(...).toBeVisible / toBeHidden / toHaveText / toBeEnabled / toHaveCount / toMatchSnapshot

### 黄金路径示例

```typescript
import { test, expect } from '@mytool/test'

test('login → onboarding → home', async ({ app }) => {
  await app.launch('com.example.app')

  await app.tap({ text: 'Sign in' })
  await app.fill({ id: 'emailField' }, 'user@example.com')
  await app.fill({ id: 'passwordField' }, 'secret')
  await app.tap({ role: 'button', name: 'Continue' })

  await expect(app.element({ text: /Welcome/ })).toBeVisible()
  await app.tap({ text: 'Skip', near: { text: 'Onboarding' } })

  await expect(app.element({ role: 'tab', name: 'Home' })).toBeVisible()
})
```

## MCP Tools（v0）

约 18 个，按 AI agent 一步一停的循环设计。详见 `docs/mcp-tools.md`（待写）。

核心点：
- `screen_describe` 一等公民，AI 每步调
- 所有交互 tool 默认带新屏幕反馈
- 失败时返回 `ExpectationFailure` 直接 feed 回 AI

## 不做的事

- ❌ 真机
- ❌ 真机相关的 tunnel / DDI / 签名
- ❌ 把 App Intents / VisualIntelligence 当读屏接口
- ❌ 假设 `XCTHIDEventGenerator` 会公开
- ❌ XPath / 坐标 selector 进 DSL 表面
- ❌ 裸 sleep API
- ❌ XCTest target 内嵌 HTTP server（模拟器场景下 host 直连模拟器，那一层多余）

## 后续路线

### v0（4 周）
1. TS 项目骨架 + core types + SDK 接口
2. 黄金路径示例 + AI prompt 范式（先 types-only，能编译）
3. Swift sim-control bridge：CoreSimulator boot/shutdown/list + simctl wrapper
4. Input bridge：iOS 26 9-arg Indigo HID 注入
5. AX bridge：`AccessibilityPlatformTranslation` 读取
6. CLI: run / repl
7. MCP server 最小集（10 个 tool）

### v1
- 录制器
- Watch mode
- 视觉兜底（Vision OCR）
- 多 sim 并发

### v2
- 寄生 Xcode 26 Automation Explorer 录制
- Foundation Models 端上 reasoning
- 视觉 snapshot diff

## 训练语料 / 风险

- iOS 26 的 9-arg Indigo 是私有 ABI，Xcode 大版本可能 break。abstraction 包住，破时降级到 XCUITest。
- 私有 framework 链接需要 dynamic load + 符号查找，避免硬链接。

## 参考

- 调研报告：见对话 history（两轮 deep dive）
- Baguette（iOS 26 私有 HID 注入实战）：https://github.com/tddworks/baguette
- Maestro 架构：https://maestro.dev/blog/maestro-re-building-the-ios-driver
- WWDC25 #344 Record, replay, and review: UI automation with Xcode
- Apple `XCUIAutomation`: https://developer.apple.com/documentation/xcuiautomation

---

## Cell 模式（运行时容器抽象）

**核心定义**：一个 **Cell** = 一个 booted simulator + 一组隔离的测试上下文（runner 端口 / trace dir / driver 实例）。simx 是薄编排层——sim 本身已经是 OS-level 隔离容器（独立 process tree + filesystem，住在 `~/Library/Developer/CoreSimulator/Devices/<UUID>/`），simx 不重做 isolation，**只管 udid + 端口 + 路径 + 生命周期**。

### Docker 类比映射

| Docker | simx Cell |
|---|---|
| Image | App bundle + test flow（`.test.ts` 文件 + 依赖） |
| Container | **Cell**（1 SimDevice + 1 runner port + 1 trace dir） |
| Volume | Cell 的 `.simx/cells/<id>/` trace + app sandbox dir |
| Network | runner HTTP port（base 22087 + cell index） |
| Compose | matrix run 配置（v1.1） |
| Logs | trace.jsonl + screenshots + xcodebuild log |
| Stats | cell health metric（v1.1 TUI） |

### 四层

| 层 | 内容 | v 路线 |
|---|---|---|
| **L1 资源隔离** | 每 Cell 独立 udid + 独立 runner port（22087+i）+ 独立 `.simx/cells/<id>/` trace dir。共享 Xcode toolchain / simx binary | **v1**（单 Cell 跑，但 API + 文件结构 cell-aware） |
| **L2 操作隔离** | 控制走 HTTP/HID/XCUITest，不经 host 鼠标键盘。Cell 内 simx tap 永不泄露到别处（XCUITest/Indigo 都是 in-sim API） | **已具备**（v0.2 C1+C2） |
| **L3 视觉观察** | 每 sim 默认在自己 Simulator.app 窗口（Apple 自带能力，免费）；进阶：simx TUI 状态 + thumbnail；终极：simx 内嵌 frame streaming grid viewer | **v1**：Apple 多窗口；**v1.1** TUI 状态；**v2** 内嵌 viewer |
| **L4 并行调度** | `simx run --cells=3 file.ts` / `simx matrix file.ts --variants` / matrix report | **v1.1** |

### 性能预算

- 单 booted sim ≈ 500MB-1GB RAM + ~1 CPU core when active
- 16GB 机安全并发 3-4 sim；32GB 可到 6-8
- Cell 抽象本身 overhead ≈ 0（只是 record：udid / port / path）
- Boot 复用：cell pool 不每次重 boot，常驻 sim 池 + 测试间隔 reset app state（`simctl uninstall + install`）

### API 形态（v1 锁定）

```typescript
// 概念性，具体落到 v0.4-v0.6 plan-hot
type Cell = {
  id: string             // 'cell-0001' / 用户起名
  udid: string           // 绑定的 sim
  runnerPort: number     // 22087 + index
  traceDir: string       // .simx/cells/<id>/
  bundleId?: string      // 当前 launched app
}

// v1：只能拿 1 个 Cell
acquireCell(opts: { device?, runtime?, udid? }): Promise<Cell>
runInCell(cell: Cell, testFile: string): Promise<RunResult>

// v1.1：多 Cell
acquireCells(n: number, opts): Promise<Cell[]>
runMatrix(testFile: string, variants: Variant[]): Promise<MatrixResult>
```

### Trace 目录结构

```
.simx/
  cells/
    cell-0001/                  # 1 Cell 1 个目录
      meta.json                 # { udid, runnerPort, startedAt }
      runner/
        xcodebuild-<pid>.log
      trace/
        <case-slug>/
          steps.jsonl
          0001-screenshot.png
          failure.json
```

> 当前 v0.1 / v0.2 的 `.simx/trace/<case>/`（无 cell 前缀）会在 v0.4 retrofit 到 cell 结构。

### 不变量

1. **Cell 之间不共享 runner 进程**——每 Cell 一个 xcodebuild test 子进程
2. **Cell 不带 UI 状态机**——simx 不重做 Simulator.app；Apple Simulator.app 仍是首屏 viewer
3. **Cell 退出 = 资源完全释放**：kill xcodebuild、close port、trace dir 保留（人工清）
4. **不靠 Cell 实现 sim isolation**——Apple SimDevice 自身已是 OS-level 隔离

### 命名

`Cell`（最短、技术中性、与 Docker `container` / K8s `pod` 平行；备选 `Bottle` / `Sandbox` / `Pod` 已废）。代码：`SimxCell` / `acquireCell` / `runInCell`。

---

## A11yNode wire 协议（v0.3 C3 锁定）

> 此章节锁定 SimxRunner `GET /tree` (v0.3 C1) 的 JSON 字段集与 TS 侧
> `RunnerClient.getTree()` (v0.3 C3 加 role batch fill) 双方契约。
> 后续 v0.7+ `HostAxpTreeSource` 必须遵守同一字段集，否则破契约。

### 字段表

| 字段 | 类型 | 必填 | nullable | 含义 / 单位 | 数据源 |
|---|---|---|---|---|---|
| `rawType` | string | ✓ | ❌ | XCUIElement.ElementType 原始名 (e.g. "button", "staticText", "cell", "other") | `TreeRoute.elementTypeName(rawValue:UInt)` 手写表，与 `src/core/role.ts` 的 `XCUIElementType` 表同构 |
| `role` | `Role \| undefined` | ✗ | optional | Playwright 风格 Role；rawType 不在 KNOWN_ROLES 时省略 | TS 侧 `elementTypeNameToRole(rawType)` batch fill (v0.3 C3)；Swift wire **不**出此字段 |
| `identifier` | string | ✗ | optional | a11y identifier；空字符串时省略 | `XCUIElementSnapshot.identifier` |
| `label` | string | ✗ | optional | a11y label；空字符串时省略 | `XCUIElementSnapshot.label` |
| `value` | string | ✗ | optional | a11y value；空 / nil 时省略 | `XCUIElementSnapshot.value`（注：snapshot.value 是 `Any?`，仅当 `String` 时收录） |
| `text` | string | ✗ | optional | 显示文本；C1-C3 暂不填（XCUIElementSnapshot 无独立 text 字段） | 预留 v0.7+ AXP 路径填 |
| `bounds` | `{x,y,w,h}` | ✓ | ❌ | **logical points @1x**；root = app frame；非根 = element frame；屏幕外 / 空 frame 仍出（用 visible 判） | `XCUIElementSnapshot.frame` |
| `enabled` | bool | ✓ | ❌ | a11y isEnabled | `snapshot.isEnabled` |
| `selected` | bool | ✓ | ❌ | a11y isSelected | `snapshot.isSelected` |
| `hasFocus` | bool | ✓ | ❌ | **C1-C3 占位 false**；v0.7+ AXP host-side 真填 | placeholder false |
| `visible` | bool | ✓ | ❌ | 启发式：frame ∩ appFrame 非空 && 非零 | `TreeRoute.isVisible(frame:appFrame:)` |
| `children` | `A11yNode[]` | ✓ | ❌ | 子节点；最深 60 层；空数组允许 | `snapshot.children` 递归 |

### Role 翻译算法

1. Swift wire 出 `rawType: string`（**不**出 role；`TreeRoute.swift` line 160）
2. TS 侧 `RunnerClient.getTree()` 在 `isA11yNode` 守卫通过后调 `fillRolesInPlace(root)`：
   - 对每个节点：`role = elementTypeNameToRole(node.rawType)`
   - `rawType in XCUIElementType` 且在 `KNOWN_ROLES` → 赋 Role
   - 否则 `delete node.role`（`exactOptionalPropertyTypes` 兼容）
   - **永远以 rawType 为准**：wire 上游若误带 `role` 字段会被覆盖
3. v0.7+ `HostAxpTreeSource` 实现必须自行 fill role（或调用同 `fillRolesInPlace`）

### 单位规则

- **bounds 单位 = logical points @1x**（与 `XCUIElementSnapshot.frame` 一致）
- 非 device pixel；非 host pixel；非归一化 `[0,1]`
- selector resolver (C5 modifiers) 的几何阈值（near / below / leftOf / ...）以同单位运算
- 坐标系：左上原点；x 向右增；y 向下增（与 `CGRect` 一致）

### 截断行为

- Apple XCUITest snapshot 硬限约 ~60 层（RN / Flutter 深嵌易超）
- 超限处理：`TreeRoute.MAX_DEPTH = 60`，超过时 **静默截断** + stderr warn 单行
- **wire schema 不带 `_truncated` flag**（C1 + C3 决策：保字段集稳定；调试时看 stderr log）
- C4+ resolver 在 truncated tree 上能 / 不能找到元素是已知风险，由 `visibleElements` suggestions 兜底

### 占位字段（v0.3 not-truly-filled）

- `hasFocus`: 永远 `false`（`XCUIElementSnapshot` 不暴露）→ v0.7+ AXP 主路径补
- `text`: C1-C3 永不输出（snapshot 无独立 text 字段）→ 视后续需要

### A11yTreeSource interface（v0.3 C3）

```typescript
interface A11yTreeSource {
  getTree(): Promise<A11yNode>
}
```

Impl：

- `RunnerClient` (v0.3 C1 + C3 role fill) — XCUI snapshot via runner
- `HostAxpTreeSource` (v0.7+) — AccessibilityPlatformTranslation host-side

`RunnerClient` 通过结构等价（structural typing）自然满足，**不**显式 `implements`（避免改 RunnerClient 签名时 cascade error）。多设备 / Cell-aware 形态推 v0.4-v0.6 Cell allocator 时由 Cell 持有 source 实例，**不**在 interface 加 `cellId` 参数。

### 已知偏差（C3 锁定时记录）

- `value` 仅当 `snapshot.value` 是 `String` 时收录；其他类型（Bool / Number / NSAttributedString）C1 实现 `value as? String`，非 String 时省略
- `identifier == ""` 时省略字段（C1 line 161），意味着 "无 identifier" 与 "identifier 为空串" 不可区分（应用程序工程实践中通常等价）

---

## Selector 匹配语义（v0.3 C4 锁定）

> 此章节锁定 `resolveSelector(tree: A11yNode, selector: Selector): A11yNode | null`
> (`src/core/resolve-selector.ts`) 的匹配语义。后续 C5 加 modifiers / C6
> driver 接通 / SDK `ElementHandle.resolve` / MCP `screen_describe` 必须遵守
> 同一语义，否则破契约。

### 搜索顺序

- **DFS pre-order**：访问 self → 递归 children 左到右
- **first match wins**：找到第一个匹配的节点立即返回，停止后续遍历
- 模拟 Playwright `querySelector` / DOM Tree Walker 行为

### 4 base 匹配规则

| base | 匹配字段 | 算法 | 空字符串 |
|---|---|---|---|
| `text: string` | `node.label` ∨ `node.value` ∨ `node.text` | 严格相等 (any one hits) | 永不匹配 |
| `text: RegExp` | `node.label` ∨ `node.value` ∨ `node.text` | `.test(field)` 任一命中 (partial) | n/a |
| `id: string` | `node.identifier` | 严格相等 (case-sensitive) | 永不匹配 |
| `label: string` | `node.label` | 严格相等 (case-sensitive) | 永不匹配 |
| `role: Role` | `node.role` | 严格相等 (`===`) | n/a |
| `role: Role, name: string` | `node.role` + (`label` ∨ `value` ∨ `text`) | role 严格 + name 严格相等任一 | name='' 永不匹配 |
| `role: Role, name: RegExp` | `node.role` + (`label` ∨ `value` ∨ `text`) | role 严格 + name `.test()` 任一 | n/a |

### 字段选择理由

- **text / name 匹配 label ∨ value ∨ text 三字段**：iOS 17/18/26 cell 与 button 嵌套形态差异大（C3 决策日志已记 iOS 26 cell.label='' / cell>button.label='General'）；任一命中保跨版本健壮
- **text / name 不匹配 identifier**：identifier 是开发者键，非 user-visible 文本；text/name selector 应当是 user-facing，identifier 走专属 `{id:...}` selector
- **id 严格相等，不 contains**：identifier 是开发者写死的稳定字段，AI 测试作者期望精确匹配（`"ContinueButton"` ≠ `"continueButton"`）；fuzzy / edit-distance 在 C6 matcher 失败时 suggestions 阶段做（不污染主路径）
- **label 严格相等，不 contains**：同 id 理由；模糊匹配走 `text` selector + RegExp `/Sub.*string/`

### case sensitivity

- **所有 string 比较均 case-sensitive**（含 text / id / label / name / role）
- regex case insensitivity 由调用方在 RegExp 上加 `/i` flag 显式声明（不自动加）
- 模拟 Playwright `getByText` 同语义（`/ok/i` vs `/ok/`）

### regex 适用范围

- **仅** `Selector.text` 和 `BaseRole.name` 接受 `string | RegExp`（与 `src/core/selector.ts` type 定义一致）
- `BaseId.id` / `BaseLabel.label` 仅 `string`；RegExp 在该字段上 type 不允许
- `RegExp.test(value)` 用 partial 匹配（不自动 anchoring 为 `^...$`）；调用方需 anchor 时在 regex 内写明（`/^OK$/`）

### Modifiers（v0.3 C5 锁定）

> C5 起 `resolveSelector` 算法升级为 **collect-all → spatial filter → index pick**
> 三段管线，承接 8 个 Modifiers 字段（type 定义已在 `src/core/selector.ts`
> Modifiers）。下表锁定每 modifier 的几何语义与组合规则。C4 的 modifier
> 护栏（任一 modifier 抛 DRIVER_ERROR）已在 C5 移除。

#### 三段管线

| 段 | 输入 | 输出 | 行为 |
|---|---|---|---|
| base collect-all | `tree, selector` | `candidates: A11yNode[]` | DFS pre-order 收集所有 base 字段命中的节点（顺序与 C4 first-match 收集顺序一致） |
| spatial filter | `candidates, selector.{near, below, above, leftOf, rightOf, inside}` | `surviving: A11yNode[] \| null` | 每个 spatial modifier 独立 AND 过滤；anchor selector 递归 resolveSelector；anchor null → 整体 null |
| index pick | `surviving, selector.{first, last, nth}` | `picked: A11yNode[]` (0 or 1 elt) | 在 surviving 上索引；多个 index modifier 同时给 → last-defined-wins（first → last → nth 覆盖顺序） |

最终返回 `picked[0] ?? candidates[0] ?? null`（无 index modifier 时取 candidates[0] = C4 first-match）。

#### 关系 modifiers（geometry）

| modifier | 几何 | 阈值 / 算法 |
|---|---|---|
| `near` | centroid 欧氏距离 ≤ 100 logical points | 固定 100pt，不自适应；包含等于 |
| `below` | candidate.centroid.y **严格** > anchor.centroid.y | 主轴严格序；副轴 (x) 不约束 |
| `above` | candidate.centroid.y **严格** < anchor.centroid.y | 同 |
| `leftOf` | candidate.centroid.x **严格** < anchor.centroid.x | 同 |
| `rightOf` | candidate.centroid.x **严格** > anchor.centroid.x | 同 |
| `inside` | candidate.bounds **完整** 落入 anchor.bounds | 4 边闭包含（共边算 inside）；candidate !== anchor by reference（self 排除） |

- **bounds 单位**：logical points @1x（与 wire 协议章节一致）
- **centroid**：`{ x: bounds.x + bounds.w/2, y: bounds.y + bounds.h/2 }`
- **self 排除**：spatial modifier 永不让 candidate === anchor 自匹配（避免单元素树退化）
- **anchor null fail-fast**：spatial / inside modifier 若 `resolveSelector(tree, anchorSel) === null` → 整体 `resolveSelector` 返 null（**不**抛 ExpectationFailure；与 base 无 match 同语义）

#### 索引 modifiers

| modifier | 行为 |
|---|---|
| `nth: number` | 取 surviving[nth]；越界 → null |
| `first: boolean` | 等价 nth=0 |
| `last: boolean` | 等价 nth=len-1 |

- **排序**：surviving list 按 DFS pre-order index 排（base collect 时已建立）；**不**按 bounds 视觉序（top-to-bottom / left-to-right）。视觉排序由 spatial modifier 组合（如 `below + nth`）显式表达
- **多 index modifier**：first + last + nth 同时给 type 允许但语义冲突，C5 取 **last-defined-wins**（按 `applyIndex` 实现：先 first，再 last，最后 nth 覆盖）；同 Playwright `getByRole(...).first().last()` 行为
- **越界**：nth=10 但 surviving.length=3 → 返 null（**不**抛错）

#### 组合语义

- 多 modifier = **AND 交集**：candidate 必须同时通过每个 spatial modifier 才进 surviving；nth/first/last 在 surviving 上索引
- **不**按 modifier 在 selector object 中的 key 顺序应用（依赖 JS object key order 不稳）；按固定 SPATIAL_KEYS 序：`near` → `below` → `above` → `leftOf` → `rightOf` → `inside`，再 index
- modifier 的 anchor selector 可自带 modifier（递归 resolveSelector，类型上 self-referential）；C5 实测 ≤ 3 层够用，无显式深度限制

#### 已知行为 / 选择

- **`near` 100pt 阈值**选择：iOS Settings cell 高 ≈ 44pt / 按钮组距 ≈ 60-80pt / 屏宽 390-440pt — 100pt 涵盖"邻近"直觉且避免误命中远处。自适应阈值（基于元素 size）行为漂移不可预测，C5 拒
- **`below/above/leftOf/rightOf` 无 alignment 容差**：仅主轴严格序，副轴不约束。Playwright 同语义；想约束副轴对齐用 `inside` 或额外 `near`
- **`inside` 用 bounds 不用 DOM**：AX tree 在 iOS 17/18/26 上 cell 嵌套形态差异大（C3 决策日志），几何包含跨版本稳定；overlay / popover 场景天然支持
- **DFS index 不用 bounds 排序**：a11y DFS 顺序与视觉顺序经常不一致（iOS list cell 可能倒序）；用户要视觉顺序用 spatial modifier 组合显式表达
- **anchor null silent**：与 base 无 match 同语义返 null；C6 matcher 失败时由 driver 层填 `visibleElements` + edit-distance suggestions

#### 与 Playwright 的差异

- Playwright `near` / `below` 等是 `Locator.filter({hasText, ...})` 的语义化封装，几何阈值在 Playwright 内部不暴露；simx `near` 阈值（100pt）在 design.md 明文锁定
- Playwright `nth(n)` / `first()` / `last()` 严格独立 chainable；simx Modifiers 同 selector object 多个同给 → last-defined-wins，避免显式 chain 语法
- Playwright 的 spatial filter 基于 DOM 渲染坐标；simx 基于 a11y `A11yNode.bounds`（logical points @1x），与可见性 / hit-test 解耦

### 未来扩展占位（C6+ / v0.7+）

- **C6**：`Driver.findOne` / `findAll` / `waitFor` 接通；`SimctlDriver.tap({id|role|label|text+regex|...+modifier})` 真触发（resolver → coord（host-hid digitizer）或 → label fallback runner /tap）
- **C6 matcher**：失败时 `visibleElements` + `suggestions` (edit-distance) 填充；C4/C5 resolver 仍严格不模糊
- **v0.7+ HostAxpTreeSource**：tree source 切换，但 resolveSelector 语义不变（pure function over A11yNode）

### 已知差异（与 Playwright / v0）

- Playwright `getByText` 默认匹配 visible text；C4 `{text}` 匹配 a11y `label`/`value`/`text`（含不可见但 a11y 暴露的字段）—— v1 通过 a11y 而非视觉做底层契约
- Playwright `getByRole` 有 `accessibleName` 概念；C4 `{role, name}` 的 `name` 直接走 `label`/`value`/`text` 三字段（与 text selector 同语义）
- v0 期偏向 contains / fuzzy；v0.3 C4 锁严格匹配，模糊 / 建议留给 C6 matcher 失败时的 `suggestions` 通道（不污染主匹配路径）
