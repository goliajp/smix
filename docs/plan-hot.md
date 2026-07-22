# plan-hot — v2.6 到 C1:动画默认就是关的,而且分平台说实话

## 目标 checkpoint

**C1**:一次 `smix run` 在两个平台上都先把动画压到该平台能压到的最低,**回读校验**,
默认如此;`--animations` 恢复;`--stable` 这个名字不存在;
破坏性变更 #10 进表进 CHANGELOG(由 v2.5 的闸门强制两处一致)。

## 前置条件

```bash
git status --short                     # 期望:空
grep -rc "set_reduce_motion" crates/smix-simctl/src/lib.rs   # 期望:1(定义,零调用方)
grep -rc "window_animation_scale" crates/ android-runner/    # 期望:全 0
grep -c "waitForAnimationToEnd" docs/ai-guide/02-yaml-reference.md  # 期望:6
pgrep -fl 'runner.ts|smix run|supervise'                     # 期望:空
```

---

## 本段预先定死的四个口径(执行期不得再议)

### 口径 1 — 两个平台压到的程度不同,名字和文档都必须反映这件事

| 平台 | 机制 | 强度 | 能回读吗 |
|---|---|---|---|
| Android | `settings put global window_animation_scale 0`(+ `transition_animation_scale` / `animator_duration_scale`) | **真归零** | 能,`settings get global …` |
| iOS | `simctl spawn <udid> defaults write com.apple.UIKit UIAccessibilityReduceMotionEnabled -bool 1` | **减弱,不归零** | 能,`defaults read` |

iOS 拿不到「关」:XCUITest 在独立进程,`UIView.setAnimationsEnabled(false)` 够不到被测 app。
Reduce Motion 是 smix 单方面能给的最强杠杆。

**因此**:面向用户的一句话是「smix 默认把动画压到该平台能压到的最低」,
**不是**「smix 关掉动画」。后者在 iOS 上是假的。

### 口径 2 — 设了就要回读,读不到就报错

`simctl ui appearance` 名义 per-sim 实际全局(memory 有案),所以「设置类」的操作
在这个项目里默认不可信。每个设置写完**立刻读回来比对**,不一致就 `ExpectationFailure`,
**不静默降级** —— 静默降级正是这个开关最容易骗人的地方(它声称压了,其实没压)。

### 口径 3 — 开关叫 `--animations`,默认 false;`--stable` 不存在

- 默认(不传):压到最低
- `--animations`:保持系统原样,一个设置都不动

名字说的是机制(动画在不在),不是结果(稳不稳)。**不给 `--stable` 留别名** ——
留着就是继续承诺一个 smix 验证不了的结果。

**只加在 `smix run` 上。** 单点 verb(`smix tap` 等)不加:它们是对一个已经在跑的
runner 打一枪,谁在管设备状态不由它们决定。

### 口径 4 — 17 处文档分平台改,`waitForAnimationToEnd` 不删

`waitForAnimationToEnd` 在 **iOS 上仍然有用**(Reduce Motion ≠ 零时长),
在 Android 上才近似空操作。改文档时**不许写成「这个 verb 没用了」**。

---

## 步骤(线性,3 个)

### S1. 两个平台各自把动画压下去,并回读校验

**红(写测试)**

- 文件:`crates/smix-sdk/tests/animation_scale_parse.rs`(新)
- 纯函数先行:`smix_sdk::animation_settings_verified(read_back: &[(&str, &str)]) -> Result<(), Vec<String>>`
  —— 给一组「设置名 → 读回来的值」,判断是否全部压到位;不一致的列出来
- 三条:全 `0` 通过;有一个是 `1` 报错并点名;读回空字符串(设置不存在)报错并点名
- 跑:红

**绿(实现)**

- 文件:`crates/smix-sdk/src/android_device.rs` —— `set_animation_scales(serial, enabled)`:
  三个 `settings put global`,再三个 `settings get global` 回读,交给上面那个纯函数判
- 文件:`crates/smix-sdk/src/ios_device.rs` —— `set_reduce_motion(udid, enabled)` 接上
  已存在的 simctl 调用,同样回读(`simctl spawn … defaults read`)
- 两者挂到 `DeviceControl` 上,加一个方法 `prepare_animations(&self, id, enabled)`,
  iOS / Android 各自实现;**这是第 11 条破坏性变更还是并进 #10?** ——
  并进 #10,理由与 v2.5-C1 口径 3 同源:一次能力改动的多个面,用户要知道的是「这件事变了」
- 跑:S1 转绿

### S2. 接进 run 流程,默认生效

**红(写测试)**

- 文件:`crates/smix-cli/src/guide_gate.rs` 的 MockApp 之外,新建
  `crates/smix-adapter-maestro/tests/animations_default.rs`
- 用既有的 `runtime_mock.rs` 同款 MockApp 手法,断言:
  不传 `--animations` 时 `prepare_animations(_, false)` 被调过一次;传了则一次都不调
- 跑:红

**绿(实现)**

- 文件:`crates/smix-adapter-maestro/src/entry.rs` —— 在第 3 步 foreground **之前**插入,
  两平台都走(现在那一步是 iOS-only,动画准备不是)
- 文件:`crates/smix-cli/src/main.rs` —— `smix run` 加 `--animations`,默认 false;
  `FlowArgs` 加字段
- 跑:S2 转绿

### S3. 记录、文档、破坏性变更表

**红(写测试)**

- v2.5 的 `release_record` 闸门会因为表和 CHANGELOG 不一致而红 —— 那就是这一步的红
- 另加:`docs/ai-guide/` 里不许再出现 `--stable`(文本断言,防止它被写回去)

**绿(实现)**

- `docs/v2.md` 破坏性变更表加第 10 行,`changelog` 列填新条目的加粗短语
- `CHANGELOG.md` `### Breaking` 加对应条目,内容含:默认变了、两平台强度不同、
  `assertScreenshot` 既有 baseline 可能失配、`--animations` 是恢复开关
- `docs/ai-guide/` 17 处 `waitForAnimationToEnd` 分平台改写;
  in-scope #4 措辞改为「animation-idle(已交付)+ 动画默认压低」,**删掉时钟那半**
- `docs/v2.md` 决策日志按 §10 追加:为什么废掉 `--stable` 这个名字、
  为什么 iOS 只能到 Reduce Motion、回读校验为什么是硬要求
- 跑:`bash scripts/dev/preflight.sh`

**设备核实**(不进 checkpoint 判据,但必须做并记进决策日志):
Android 与 iOS 各起一次,确认回读校验真的通过;Android 侧每条 adb 显式 `-s emulator-NNNN`

---

## Checkpoint C1 验收

```bash
cargo test -p smix-adapter-maestro --test animations_default
cargo test -p smix-sdk --test animation_scale_parse
cargo test -p smix-cli --bin smix release_record -- --nocapture 2>&1 | grep 'release-record:'
grep -rc -- "--stable" docs/ai-guide/ crates/ | grep -v ':0' || echo "no --stable anywhere"
bash scripts/dev/preflight.sh
```

期望:

1. 前两条 `test result: ok. … 0 failed`
2. 第三条读作 `10 breaking changes, both lists agree · 8 behaviour changes …`
3. 第四条输出 `no --stable anywhere`
4. `preflight: clean`

## 完成后动作

1. `mv docs/plan-hot.md docs/plan-history/v2.6-c1-hot.md`
2. 生成 C2 热计划(MCP `smix_diagnose` + 撤回 session 工具 + 错误恢复契约),
   附加 context:`smix-mcp` 13 个 tool 的错误全部走 `e.to_prompt()`,落点已在
