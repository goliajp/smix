# plan-hot — v2.1 到 C3:runner state 与 capsule state 进 store

## 目标 checkpoint

C3:`.smix/runner/state.json` 与 `.smix/capsule/{udid}.state.json` 不再被写入。
runner state 的两个写入方(iOS `runner.rs` 与 Android `runner_android.rs` 各有一份
重复的 `state_path()`)合并为一处;两者按平台分键,不再互相覆盖。所有丢弃的写入、
删除与读取变成具名错误或具名缺失。

## 前置条件

```bash
cargo test -p smix-store -p smix-simctl   # C1/C2 全绿
```

## 步骤(线性)

### S1. runner state 进 store,两个写入方合一

**红**
- 文件:`crates/smix-cli/tests/runner_state_store.rs`
- 断言:
  - 写入后 `.smix/runner/state.json` **不存在**(旧文件不再被创建)
  - 旧 `state.json` 存在时,首次读取仍能取回其内容(导入路径)
  - **iOS 与 Android 的 state 互不覆盖**:先写 iOS 再写 Android,两者都读得回
    (今天两侧写同一个文件,后写的赢,而且 Android 那次写是 `let _ =` 丢错误的)
  - 损坏的存量 state → 具名错误,不是 `None`

**绿**
- 文件:`crates/smix-cli/src/runner.rs`、`crates/smix-cli/src/runner_android.rs`
- API:`runner_state::read(root, platform)` / `write(root, platform, &state)` /
  `clear(root, platform)`,单一实现,两侧共用
- 关键点:
  - key 为 `one:runner-ios` / `one:runner-android` —— 每平台一个 runner,
    与 `down` 无参数(iOS)和 `down --device`(Android)的现有语义完全一致
  - `runner_android.rs` 删除自己那份 `state_path()`
  - `read_state` 的 `.ok()?` 两处、`write` 的 `let _ =` 两处、`remove_file` 的
    七处一律改为传播或具名忽略(带理由的 `if let Err(e) = ... { eprintln!(...) }`)

**重构**
- 若两侧仍有重复的 pid/端口探测逻辑,一并收拢

### S2. capsule state 进 store

**红**
- 文件:`crates/smix-cli/src/capsule.rs` 内既有测试改造 + 新增
- 断言:
  - `up` 后 `.smix/capsule/{udid}.state.json` **不存在**
  - 两个不同 udid 的 capsule 互不干扰(它今天已是按 udid 分文件,行为须保持)
  - 既有的 `no_capture` 向后兼容测试(缺字段默认 false)仍通过 —— 值仍是 JSON,
    serde 语义不变

**绿**
- 文件:`crates/smix-cli/src/capsule.rs`
- 关键点:
  - capsule 是**按 udid 的记录**,进 `Namespace`(不是 singleton)——
    与 runner state 的"每平台一个"是不同形态,不要套同一个模子
  - 今天的 capsule 写入是全仓唯一原子的(tmp + rename)且全程 error-checked;
    迁移后不得降级为丢弃错误

## Checkpoint C3 验收

```bash
cargo test -p smix-cli -p smix-store -p smix-simctl
grep -rn 'state.json' crates/smix-cli/src/ | grep -v 'import\|legacy\|//' | wc -l
```
期望:
- 测试全绿
- 生产代码里写 `state.json` 的地方为 **0**

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.1-c3-hot.md`
2. 生成新 `plan-hot.md`(到 C4:smix-simctl 的三处持久化)
