# C7 调研 — XCUITest 遮挡感知所需 z 序(或等价遮挡信号)是否可得

> 研究先行 checkpoint（v2.8-C7）。回答 EXT1 #4 defer 过的开放问题：
> **「从 XCUITest snapshot / 私有 API 能不能可靠拿到 z 序（或等价的遮挡判定信息），
> 用来在 `tap_landed_within` 地基上补上遮挡判定？」**
>
> 全程 read-only：读源码 / 读 Apple 文档 / `nm`·`strings`·Obj-C `__objc` 段静态枚举私有二进制。
> 不 edit 实现代码、不跑设备、不起 runner。
>
> **两条调研轴**（出处 `SmixRunnerUITests.swift:1403-1406`：iOS 自身 act 期按 z 序路由触摸，
> 但 runner 走的 snapshot 不携带它）：
> - **轴 A — 静态 snapshot**：runner 已经在walk 的 `XCElementSnapshot` 树上，有没有携带 z 序 / 遍历序 / frontmost / occluded 的 ivar 或 KVC 可达属性。
> - **轴 B — act 期 hit-test**：合成触摸前后，有没有一个 act 期私有 API 能回答「该点最前响应者是不是所 aim 元素」，且不复用被拒的 `isHittable` 语义。

## Falsification rubric

**先于收证据钉死**。每条判据的 `Evidence:` 槽此刻留空，证据在 `## Evidence` 段回填，
证明 verdict 非事后合理化。

### 轴 A（静态 snapshot）

- **判 `OBTAINABLE-A` 的充分证据**：`XCElementSnapshot`（或其 KVC 可达属性）上存在一个能确定
  「某 AX 可达元素在某点是否被前层覆盖 / 是否为该点最前元素」的 ivar 或 selector，
  且可用现成 `value(forKey:)` / dlsym 范式（`TreeRoute.swift:37-40` 的 `_hasKeyboardFocus` 读法）
  在 §9#6 私有符号不变量内读出。
  - Evidence: __(空)__
- **判 `NOT-OBTAINABLE-A` 的充分证据**：枚举出的 `XCElementSnapshot` **完整 ivar+selector 全表**里
  **无**任何遍历序 / z-index / z-position / frontmost / hit-order / occluded 语义项
  （no-ceiling-words：负向结论必须附穷尽枚举表，不许用「结构性不可能」hand-wave）。
  - Evidence: __(空)__

### 轴 B（act 期 hit-test）

- **判 `OBTAINABLE-B` 的充分证据**：存在一个 act 期私有 API（私有 `elementAtPoint` / hit-test 入口 /
  XCUICoordinate 等）在合成触摸前后能回答「该点最前响应者是不是所 aim 元素」，
  **且不复用**被拒的 `isHittable` 语义。
  - Evidence: __(空)__
- **判 `NOT-OBTAINABLE-B` 的充分证据**：唯一（或全部）候选归结为 `isHittable` 语义，
  撞上两条已记拒因（① AX 可达但视觉被盖时恒 false = see-through tap 的语义就是穿过去；
  ② v1.0.27 破了 QA-overlay 断言，见 `lib.rs:1424-1428` + `wire-format.md:81`）。
  - Evidence: __(空)__

### isHittable 对照闸（贯穿两轴）

- `isHittable` **默认不是答案**。任何轴把它当候选浮出，必须逐条对照上述两条已记拒因，
  不许把 `isHittable` 悄悄当「z 序」塞回来。
  - Evidence: __(空)__

### verdict 判定

- `OBTAINABLE`：轴 A 或轴 B 至少一条给出**非 isHittable、可读、覆盖 smix 关切的遮挡类**的信号。
- `NOT-OBTAINABLE`：两条轴都拿不到。
- `PARTIAL`：一条轴可得、另一条不可得（如 act 期 hit-test 可判某类遮挡但静态 snapshot chain 不可判）。

## Evidence

所有 `nm`/`strings`/`otool -s __TEXT __objc_methname` 均对本机（Xcode 26.6 / Build 17F113）以下二进制静态枚举，read-only：

- `XSUP` = `/Applications/Xcode.app/Contents/Developer/Platforms/iPhoneSimulator.platform/Developer/Library/PrivateFrameworks/XCTAutomationSupport.framework/XCTAutomationSupport`（`XCElementSnapshot` 类定义所在，Mach-O universal x86_64+arm64，`nm -a | grep -ci XCElementSnapshot` = 263）
- `XCUI` = `/Applications/Xcode.app/Contents/Developer/Platforms/iPhoneSimulator.platform/Developer/Library/Frameworks/XCUIAutomation.framework/XCUIAutomation`（`XCUIElement` / `XCAXClient_iOS` / hit-test 引擎所在）
- `XCTC` = `.../PrivateFrameworks/XCTestCore.framework/XCTestCore`

### 轴 A（静态 snapshot）证据 → `NOT-OBTAINABLE-A`

**`XCElementSnapshot` 完整 ivar 全表**（`nm -a $XSUP | grep '_OBJC_IVAR_$_XCElementSnapshot'`，35 个，穷尽）：

```
_accessibilityElement        _frame                     _placeholderValue
_activationPoint             _generation                _selected
_additionalAttributes        _hasFocus                  _systemAutomationProperties
_application                 _hasKeyboardFocus          _title
_children                    _hasPrivilegedAttributeValues  _traits
_dataSource                  _horizontalSizeClass       _userTestingAttributes
_disclosedChildRowAXElements _identifier                _value
_displayID                   _interfaceOrientation      _verticalSizeClass
_elementType                 _isMainWindow              _windowContextID
_enabled                     _isTruncatedValue
_eventSynthesisFrame         _label
_faultedInProperties         _localizableStringInfo
_parent                      _parentAccessibilityElement
```

- **无任何** `_zIndex` / `_zPosition` / `_zOrder` / `_frontmost` / `_topmost` / `_hitOrder` /
  `_traversalIndex` / `_occluded` / `_coveredBy` / `_layerOrder` 语义项。逐字 sweep 全 XCUIAutomation 二进制
  （`nm -a $XCUI | grep '_OBJC_IVAR_$_' | grep -iE 'zindex|zposition|zorder|frontmost|occlud|topmost|traversalIndex|hitOrder|layerOrder|depthOrder'`）**零命中**。
- **唯一沾「顺序 / 深度」的 selector**（`strings -a $XSUP | grep -iE 'depth|traversal'`）是 `depth` / `maxDepth` /
  `traverseFromParentsToChildren` —— 全是 snapshot **请求参数**（树 DFS 遍历深度上限 / 父→子走向），是
  **树结构遍历序**，**不是视觉 z 序**。它们不在 ivar 表（`nm | grep _OBJC_IVAR.*depth` 零命中），仅为请求参数字符串。
- `_frame` / `_eventSynthesisFrame` / `_activationPoint` 是几何；`_isMainWindow` / `_windowContextID` /
  `_displayID` 是窗口/显示身份（粗到窗口级，非同窗内 sibling z 序）；`_generation` 是 snapshot 世代计数器，非 z 序。
- `_additionalAttributes` / `_systemAutomationProperties` / `_userTestingAttributes` 是不透明 AX 属性字典 ——
  键是 AX 属性值（label/value/traits 之类），**不携带兄弟绘制顺序**；它们是内容属性面，不是几何 z 序面，
  且内容随 app 而变，不能作为「该点被谁盖」的稳定信号。
- 对照 `TreeRoute.swift:273`「Snapshots are dead frames」+ `lib.rs:1422-1423`「snapshot carries no z-order」——
  枚举实证了这个论断：snapshot 是死帧，遮挡关系从来没被物化进任何 ivar。
- **`value(forKey:)` 范式核对**（`TreeRoute.swift:37-40` 读 `_hasKeyboardFocus` 的现成模板）：
  即便想读，也**无标的可读** —— 表里没有承载 z 序的 ivar，KVC key 无处指向。范式可用但目标不存在。

→ **轴 A 判 `NOT-OBTAINABLE-A`**：完整 35 ivar 全表 + 全二进制 ivar sweep 无任何遮挡/z 序语义项（穷尽枚举，no-ceiling-words 举证责任已尽）。

### 轴 B（act 期 hit-test）证据 → 归结为 `isHittable`，加一条非 isHittable 但对目标遮挡类失明的候选

XCUIAutomation 里**确实存在**遮挡感知机器（`strings -a $XCUI` 命中）：

```
Occluding element %@ has been encountered %lu times. Now treating its frame as %@
Recompute visible frame by excluding frames of occluding elements %@
Unable to compute coordinates for gesture after %d attempts, final visible/unoccluded frame was %@.
Unable to find unoccluded area to perform event.
```

但把它的 API 面拆开，它**整体归约到 `isHittable` 语义**：

- **`XCUIHitPointResult`**（`_OBJC_IVAR_$_XCUIHitPointResult`）只携两个 ivar：`_hitPoint`（CGPoint）+ `_hittable`（BOOL）。
  这就是 hittability 结果对象 —— **没有更丰富的 z 序 / frontmost-element 字段**。构造 selector `initWithHitPoint:hittable:`。
- 遮挡计算引擎 selector（`otool -s __TEXT __objc_methname $XCUI`）：`_hittableElementsInSnapshots:` /
  `suggestedHitpoints` / `visibleFrameInTopLevelElement` / `_hitPointByAttemptingToScrollToVisibleSnapshot:error:`。
  它们**从 snapshot 几何按「排除 occluding 兄弟 frame」推算** hittable 点 / visible frame，产物就是 `hittable` BOOL +
  可见 frame。即上面那几条 "Recompute visible frame by excluding occluding elements" 日志的来源。
- `isHittable` 属性（`otool` 命中 `TB,R,GisHittable,V_hittable`，backing ivar `_hittable`）就是这套引擎的对外布尔。
- **Apple 文档佐证**：`XCUIElement.isHittable` 在元素「不存在 / offscreen / **被另一元素覆盖**」时返回 false
  （Apple Developer 文档 + 开发者论坛 thread 720155）—— 即 **Apple 官方暴露的 z 序遮挡答案就是 `isHittable`**。

→ 这条路撞上两条**已记拒因**（`lib.rs:1424-1428` + `wire-format.md:81`）：
  ① AX 可达但视觉被盖时 `isHittable`=false，而 smix 的 see-through tap 语义**就是刻意穿过去**（`SmixRunnerUITests.swift:1410-1413` 对 `seeThrough` 显式跳过 `isHittable`）；
  ② v1.0.27 `isHittable` 破了 QA-overlay 断言（floating overlay 下 hittable=false 但元素可见可断言，`wire-format.md:81`）。
  **判 `NOT-OBTAINABLE-B`（就 isHittable 引擎而言）**。

**唯一非 isHittable 的 act 期候选** —— `-[XCAXClient_iOS accessibilityElementForElementAtPoint:error:]`
（即 DTX 私有 `_XCT_requestElementAtPoint:reply:`，`strings` 在 `$XCTC`+`$XCUI` 均命中）：

- 它把「点」发给 **app 侧 accessibility 服务器做真 AX hit-test**，回该点解析出的 AX 元素 = 该点**最前的 AX 可命中元素**。
  这**是** z-order-aware（走 app 真实渲染层的 `accessibilityHitTest:` 等价），**且不是** `isHittable` 的几何 snapshot 启发。
  它满足 rubric「回该点最前响应者、非 isHittable」的字面条件 —— 对 **a11y 树里存在的**遮挡层，它能判「最前元素 ≠ 所 aim 元素」。
- **但它对 smix 关切的遮挡类失明**：smix 反复记的遮挡是「**something transparent to the a11y tree**」
  （`lib.rs:1421` scrim / `07-errors.md:131-132`「an element covered by something transparent to the a11y tree」/
  `04-actions.md:48-50`）。a11y 透明的 scrim **不在 AX 树里** → AX hit-test 直接穿过它、回下面的 button
  → 与静态 snapshot 几何引擎**同样看不见**（两者都是 a11y 派生）。对 a11y **存在**的遮挡层它能判，
  但那类 smix 本就能用 isHittable 判（且刻意拒了）。
- 且它是 **act 期 live 查询**（要 aim + 一次跨进程 AX 往返），**不是** `tap_landed_within` 走的**静态 snapshot chain**
  上的字段 —— 把它接进来是一格**独立新能力**（act 期 AX-hit-test 探针），不是现有 chain 判定的扩展。
  `XCUICoordinate` 无公开 `element(atPoint:)`；Apple 公开面只有 `isHittable`（同上被拒）。

→ **轴 B 综判 `PARTIAL-B`**：z 序遮挡信号在 act 期**存在**，但（a）XCUITest 自带引擎 = 被拒的 `isHittable`；
  （b）唯一非 isHittable 候选 `accessibilityElementForElementAtPoint:` 是 act 期独立能力，且对 smix 记的
  a11y-透明遮挡类**失明**（该类对任何 a11y 派生机制都不可见）。

### isHittable 对照闸

已在轴 B 逐条对照：XCUITest 的遮挡引擎（`XCUIHitPointResult`/`_hittableElementsInSnapshots:`/`visibleFrame...`）
全部塌缩为 `_hittable` BOOL = `isHittable`，撞两条拒因，未被当 z 序塞回。非 isHittable 的候选单独评估并标其对目标遮挡类的失明。

### 综合

- **对 smix 实际记的遮挡类（a11y-透明 scrim）**：轴 A（snapshot）与轴 B（AX hit-test）**都失明** —— 不是懒得找，
  是该类遮挡对任何 a11y 派生机制都不留痕（穷尽枚举证明，非 hand-wave）。
- **对 a11y-存在的遮挡层**：轴 A 仍不可得（snapshot 无 z 序 ivar）；轴 B 可得，但 = 被拒的 `isHittable`，
  或 = 一格独立的 act 期 AX-hit-test 新能力（非 `tap_landed_within` chain 扩展）。
- 故：静态 snapshot chain（`tap_landed_within` 的地基）**结构性拿不到** z 序（轴 A NOT-OBTAINABLE，已穷尽举证）；
  act 期存在非 isHittable 的 z 序信号但只覆盖 a11y-存在遮挡、且是独立能力（轴 B PARTIAL）。**一轴不可得、一轴部分可得 = PARTIAL。**

VERDICT: PARTIAL — 静态 snapshot（轴 A）穷尽枚举无 z 序 ivar、结构性拿不到；act 期（轴 B）存在非 isHittable 的 z 序信号（`accessibilityElementForElementAtPoint:`）但仅覆盖 a11y-存在遮挡、对 smix 记的 a11y-透明 scrim 失明，且是独立 act 期能力而非 `tap_landed_within` chain 扩展。

