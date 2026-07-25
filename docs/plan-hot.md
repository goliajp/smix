# plan-hot — v2.13 到 C4：MCP 会话内选设备、自己把 runner 起起来

## 目标 checkpoint

C4：一个刚连上的 MCP 客户端，不靠任何预先设好的环境变量、不靠有人在别的终端跑过
`capsule up`，就能列出设备、挑一台、把 runner 起来、开始驱动，结束时释放。
即 `smix_devices` / `smix_use` / `smix_release` 三个 tool 落地，`SMIX_UDID` 从**必需降为默认值**。

## 前置条件

```bash
sed -n '560,580p' crates/smix-mcp/src/main.rs      # 现行：启动时读 env 绑一台，之后不可变
grep -n "^mod runner;" crates/smix-cli/src/main.rs # 生命周期埋在二进制 crate 里
cargo test -p smix-cli --bin smix > /dev/null 2>&1; echo $?
```

## 步骤（线性，3 步）

### S1. 把 runner 生命周期从二进制里提出来

**红（写测试）**
- 文件：`crates/smix-capsule/tests/lifecycle.rs`
- 断言：新 crate 暴露 `xcodebuild_argv` / `runner_env` / `health_ok` / `up` / `down`，
  且 `cargo metadata` 显示 `smix-cli` 与 `smix-mcp` **都**直接依赖它
  （两个消费方共用一份实现，是这次搬迁唯一的理由）

**绿（实现）**
- 新 crate `smix-capsule`：把 `smix-cli/src/runner.rs` + `runner_state.rs` 整体搬过去，
  `smix-cli` 改为 `use smix_capsule::…`，既有测试原样跟着搬（**不重写测试**，
  搬迁若改了行为，跟着搬的测试就是发现它的地方）
- 关键点：**不 shell 出去调 `smix` 二进制**。MCP 直接调 CLI 是把能力留在消费方那侧，
  与 §12.1 相悖；且 MCP 与 CLI 各自演进时，shell 调用的参数形会静默漂移

**重构**
- `smix-cli` 只留 `mod` 声明与 re-export，不留副本

### S2. MCP 的会话内设备状态

**红（写测试）**
- 文件：`crates/smix-mcp/src/session.rs`（`#[cfg(test)] mod tests`）
- 断言（纯逻辑，无设备）：
  - 未绑定时调用感知/操作 tool → 错误信息里点名 `smix_use`，而不是一句连不上
  - `bind` 之后 `current()` 返回该 UDID；再 `bind` 另一台 → 换绑且旧端口被记为待释放
  - `release` 后回到未绑定，且再调用感知 tool 仍给同一条点名 `smix_use` 的错误

**绿（实现）**
- `SessionState`：`Mutex<Option<Bound{udid, port}>>`（rmcp 的 handler 拿 `&self`，
  所以状态必须是内部可变的）
- `SMIX_UDID` / `SMIX_RUNNER_PORT` 降为**初始默认值**：有就预绑，没有就未绑定，
  不再是「没有就废掉一半 tool」

**重构**
- 无

### S3. 三个 lifecycle tool + 设备实证

**红（写测试）**
- 文件：`scripts/dev/v2.13-c4-mcp-session-e2e.sh`
- 断言（真 MCP over stdio，无 `SMIX_UDID`）：
  1. `initialize` 握手成功
  2. `tools/list` 含 `smix_devices` / `smix_use` / `smix_release`
  3. 未绑定时调 `smix_tree` → 错误文本含 `smix_use`
  4. `smix_devices` → 返回的列表含本仓自有 sim 的 UDID
  5. `smix_use` 该 UDID → 成功；此后 `smix_tree` 返回真树
  6. `smix_release` → 成功；runner 收掉，无孤儿 xcodebuild

**绿（实现）**
- 三个 tool 落在 S1 的库上；`smix_use` 幂等（已绑同一台就直接返回）

**重构**
- 无

## Checkpoint C4 验收

```bash
cargo test -p smix-capsule > /tmp/c4a.log 2>&1; echo $?; grep -E "test result" /tmp/c4a.log
cargo test -p smix-mcp > /tmp/c4b.log 2>&1; echo $?; grep -E "test result" /tmp/c4b.log
bash scripts/dev/v2.13-c4-mcp-session-e2e.sh > /tmp/c4.log 2>&1; echo $?; tail -1 /tmp/c4.log
```
期望：两条 `cargo test` 退出 0 且 `N passed` 中 N > 0；e2e 退出 0 且末行 `C4-MCP-SESSION-PASS`。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.13-c4-hot.md`
2. 生成新 `plan-hot.md`（到 C5：plugin 骨架 + marketplace + 就绪 hook）
