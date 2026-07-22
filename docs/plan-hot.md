# plan-hot — v2.7 到 C4:剩下的两条都在 runner 侧

## 目标 checkpoint

**C4**:EXT1 九条全部有归宿。剩余两条都不是 CLI 能透传的东西,都要动 runner 的语义:

- **#2 观察按住期间**(他们排第 2 优先)
- **#8 `capsule up` 不重启应用**

## 前置条件

```bash
git status --short                     # 期望:空
bash scripts/dev/preflight.sh          # 期望:preflight: clean
ssh mini 'cd workspace/goliajp/smix && cargo test --workspace 2>&1 | grep -c "^test result: FAILED"'
# 期望:0
```

## 已经查清、不必重查的事实

- **按住本身已经有了**:`longPressOn: { id, duration }` 走 `element.press(forDuration:)`,
  真的按住。EXT1 的 #2 缺的**只是「期间能不能看」**
- **并发探测已做(2026-07-23,mini / iPhone 17 Pro / iOS 26.5),结论是决定性的**:
  按压进行中(占用 0.00–4.31s),`/health` 发于 +0.00s、**+0.01s 就返回**;
  而 `/tree` 发于 +1.01s、**+4.38s 才返回** —— 排在按压结束之后。
  **HTTP 层是并发的,凡碰 XCUITest 的路由都排在进行中的手势后面。**
  所以 EXT1 的「外部截图落进按压窗口」**在机制上不可能**,他们看到的
  「the press had already ended」不是按压太快,是截图被挡到了按压之后。
- **因此 #2 只剩较贵的那条路**:handler 自己在按住窗口内取帧。
  而 `element.press(forDuration:)` 是**同步阻塞**的,handler 在它里面也取不了 ——
  要先确认 `SmixEventRecord` 的 hold(offset 时间轴)提交后
  `synthesize` 是**立即返回**还是**阻塞到手势结束**。前者才有窗口可用。
  **这是 C4-S1 的第一步,不是实现的第一步**
- **`capsule up` 的重启在 runner 侧**:XCUITest 绑定 bundle 时 `.launch()`。
  `crates/smix-cli/src/runner.rs` 全文没有 launch 字样,CLI 透传不到
- **性能基线**(mini / iPhone 17 Pro / iOS 26.5):`GET /tree` 68ms,
  `POST /tap-at-norm-coord` 466ms,10 次 burst 917ms

---

## 本段预先定死的两个口径

### 口径 1 — #2 先查「runner 能不能并发服务」,再决定形态

两种形态成本差一个数量级:

~~runner 能并发 → 只需更长的 duration,外部截图自己落进窗口~~ ——
**已被探测排除**(见上)。EXT1 说这就够,但机制上做不到。

只剩:handler 自己在按住窗口内取帧并返回,那是新的响应形状 + 图像传输;
且它成立的前提是 synthesize 提交后立即返回。

**先测再定,不先按想象实现** —— C1 的语义就是这样被设备否掉的,
而这一条的并发探测同样推翻了「加个 duration 就够」的省事路线。

### 口径 2 — #8 要先回答「附着是什么意思」

`capsule up` 现在等于「起 runner 并把它绑到 bundle」,而绑定就会 launch。
**不改 launch 行为、只加一个 flag,是把语义问题伪装成参数问题。**

要回答的是:runner 能不能绑到一个**已经在跑**的 app 而不重启它。
XCUIApplication 有 `activate()` 与 `launch()` 之分 —— 前者不重启。
先确认 runner 侧当前用的是哪个、以及改成 activate 会不会破坏它别的保证。

---

## 步骤(线性,2 个)

### S1. #2 —— 先测并发,再按结论实现

**红**:一条断言「按住期间能取到与静止时不同的画面」。测法由上面的并发探测结论决定。

**绿**:按结论的形态实现,并在 docstring 写明**另一条路为什么没选**。

### S2. #8 —— 先确认 activate 与 launch 的差别,再决定

**红**:`capsule up` 两次,中间导航到别的屏;第二次之后断言屏幕没被重置。

**绿**:runner 侧改绑定语义;若发现 activate 会破坏别的保证,
**记下来并停**,不为了闭一条反馈项而牺牲一条已有保证。

---

## Checkpoint C4 验收

```bash
cargo test -p smix-cli --bin smix guide_gate -- --nocapture
bash scripts/dev/preflight.sh
ssh mini 'cd workspace/goliajp/smix && cargo clippy --workspace --all-targets && cargo test --workspace'
```
外加:`.claude/dogfood/2026-07-22-ext1-response.md` 的「准备怎么做」一节
已改为实际结果,九条各有归宿(已修 / 已做 / 明确不做并写明理由)。

## 完成后动作

1. `mv docs/plan-hot.md docs/plan-history/v2.7-c4-hot.md`
2. v2.7 出口验收成立 → 回 `docs/v2.md` 看 v2 是否只剩发布本身
