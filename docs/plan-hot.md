# plan-hot — v2.7 到 C1:tap 说出它打中了什么

## 目标 checkpoint

**C1**:`tapOn` 的成功含义从「我派发了一次触摸」变成「我打中了我匹配的那个元素」。
打中别的东西 → 失败,并说出**瞄的是谁、打中的是谁**。

## 前置条件

```bash
git status --short                     # 期望:空
bash scripts/dev/preflight.sh          # 期望:preflight: clean
grep -c "OkEnvelope" crates/smix-runner-client/src/lib.rs   # tap 仍收空信封
pgrep -fl 'runner.ts|smix run|supervise'                    # 期望:空
```

---

## 本段预先定死的四个口径(执行期不得再议)

### 口径 1 — 判据是「同一个元素」,不是「坐标落在框里」

主机已经解析出一个元素(有 identifier / label / frame)。runner 派发后回报**那一点上的元素**。
判据是两者**是同一个**,不是「坐标在框内」——后者对被遮挡的情况恒真,
而遮挡正是 #4 报的现象之一。

比对顺序,预先定死:
1. 两边都有非空 `identifier` → 比 identifier
2. 否则两边都有非空 `label` → 比 label
3. 否则比 frame(容差 1pt,浮点)
4. 以上都无法比 → **判为「无法确认」而不是「通过」**,并在失败文本里说明为什么无法比

第 4 条是这条设计的重点:**不能比就不叫通过**。

### 口径 2 — 语义先看设备再定死

「那一点上最深的元素」与「那一点上真正接收事件的元素」不是一回事。
XCUITest 没有公开的 hitTest,能拿到的是快照树的几何包含关系。

因此 **C1 先在设备上把「最深包含者」取出来看它跟实际响应者差多少**,再决定
`tapOn` 拿哪个当判据。**不允许先按想象实现再去验证** —— 这一段的风险表就写着这条。

### 口径 3 — 换判据是破坏性变更,并且要有过渡

今天报成功的流,明天可能报失败 —— **那正是要的效果**,但不能一声不响地换。

- 默认:不一致 → 失败
- `SMIX_TAP_HIT_MISMATCH=warn`:降级为警告,给存量流一个过渡窗口
- 进破坏性变更表(#11)+ CHANGELOG,由 v2.5 的闸门强制两处一致

**不做**「默认警告、以后再改成失败」——那是把这次改动的价值推给下一次。

### 口径 4 — 比对逻辑是纯函数,住在 driver

判据可能错;错了要能被红。所以「瞄的是 A、打中的是 B,该不该判失败,失败怎么说」
必须是一个不碰网络、不碰设备的函数,单测钉住。runner 侧只负责**如实回报那一点上是谁**。

---

## 步骤(线性,3 个)

### S1. 比对判据先有,并且能红

**红(写测试)**

- 文件:`crates/smix-driver/tests/tap_hit_verdict.rs`(新)
- 纯函数 `smix_driver::tap_hit_verdict(aimed: &HitElement, hit: Option<&HitElement>) -> TapHitVerdict`
- 用例(每条对应口径 1 的一行):
  - identifier 相同 → `Confirmed`
  - identifier 不同 → `Missed`,失败文本同时含两个 identifier
  - 双方 identifier 空、label 相同 → `Confirmed`
  - 双方 identifier 与 label 都空、frame 在 1pt 内 → `Confirmed`
  - 双方全空且 frame 差 20pt → `Missed`
  - `hit == None`(runner 说那一点上没有元素)→ `Missed`,文本说「那一点上什么都没有」
  - **双方都无可比字段** → `Unconfirmable`,**不是 `Confirmed`**
- 跑:红

**绿(实现)**

- 文件:`crates/smix-driver/src/lib.rs` —— `HitElement` / `TapHitVerdict` + 判据函数
- 跑:S1 转绿

### S2. 让 runner 如实回报那一点上是谁

**红(写测试)**

- 文件:`swift-bridge/Tests/SmixRunnerCoreTests/HitAtPointTests.swift`(新)
- `SmixRunnerCore` 侧加纯函数:给一棵已有的快照节点树 + 一个点,返回**最深的包含者**
  (与 `TreeRoute` 的 `nodeToDict` 用同一份节点结构,不另造一棵树)
- 三条:命中最深子节点;点在父节点内但不在任何子节点内 → 返回父;点在树外 → nil
- 跑:`swift test`,红

**绿(实现)**

- `SmixRunnerCore` 实现该函数;`SmixRunnerUITests` 的 `tapAtCoordHandler`
  在 synthesize 之后调用它,把结果放进响应
- 文件:`crates/smix-runner-wire` —— `/tap-at-norm-coord` 的响应从空信封变成带
  `hit: Option<HitElement>`;`smix-runner-client` 的 `tap_at_norm_coord` 返回它
- 跑:`swift test` 绿 + `cargo check` 绿

### S3. 接进 driver,记录,过渡开关

**红(写测试)**

- `crates/smix-driver` 的单测:`IosDriver::tap` 在 `hit` 与 aimed 不一致时返回
  `ExpectationFailure`,文本含两个元素;设 `SMIX_TAP_HIT_MISMATCH=warn` 时不失败
- 跑:红

**绿(实现)**

- `IosDriver::tap` 接上判据;失败走 `ExpectationFailure`,`code` 用既有的
  `ElementNotFound` 还是新码 —— **新码**,因为「找不到」与「找到了但没打中」
  对读的人是两件事
- 破坏性变更 #11 进 `docs/v2.md` 表 + CHANGELOG(闸门强制)
- `docs/ai-guide/04-actions.md` §Default tap 补一段:tapOn 成功的含义是什么
- `docs/v2.md` 决策日志按 §10 记:口径 1 第 4 条(不能比就不叫通过)、口径 3(为什么不默认警告)
- 跑:`bash scripts/dev/preflight.sh`

**设备核实**(不进 checkpoint 判据,写进决策日志):
口径 2 的语义对照 —— 最深包含者 vs 实际响应者;
以及用 EXT1 报的那个现象复现一次(tap 报成功而应用没收到)

---

## Checkpoint C1 验收

```bash
cargo test -p smix-driver --test tap_hit_verdict
cd swift-bridge && swift test --filter HitAtPoint
cargo test -p smix-cli --bin smix release_record -- --nocapture 2>&1 | grep 'release-record:'
bash scripts/dev/preflight.sh
```

期望:前两条 `0 failed`;第三条读作 `11 breaking changes, both lists agree`;
第四条 `preflight: clean`。

## 完成后动作

1. `mv docs/plan-hot.md docs/plan-history/v2.7-c1-hot.md`
2. 生成 C2 热计划(#2 / #3 —— 过程中观察),附加 context:
   **#3 的 428ms 要自己复量一次**,不照抄消费方的测量
