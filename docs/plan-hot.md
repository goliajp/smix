# plan-hot — v2.4 到 C2:单点 verb 也能拿到注册表那一级

## 目标 checkpoint

**C2**:`docs/guide-executability.md` 的 **N1 行从 `broken` 转 `runs`**,并且装回缺陷时闸门会重新变红。
`05-cli.md` §Environment-variable precedence 写的四级链,对**每一个会拨 runner 的子命令**都成立。

## 前置条件

```bash
test ! -f docs/plan-hot.md || true   # 本文件即热计划,此条在生成时成立
git status --short                   # 期望:C1 的产物已提交或在工作树里,无其它人的改动
cargo test -p smix-cli --bin smix guide_gate 2>&1 | grep 'test result:'
# 期望:ok. 13 passed
grep -c '| N1 |.*| broken |' docs/guide-executability.md
# 期望:1 —— 缺陷仍在
bash scripts/dev/preflight.sh
# 期望:最后一行 preflight: clean
```

---

## 本段预先定死的三个口径(执行期不得再议)

### 口径 1 — 改实现,不改文档

冷计划把「改实现还是改文档」留给 C1 给依据后定。**C1 的依据出来了,答案是改实现。**

C1 的 probe 问的是 clap 命令树,答案是:`smix run` 四级齐全(`--runner-port` 带
`env = "SMIX_RUNNER_PORT"`,clap 在 `run_port` 看到之前就把 env 并进 flag;`--device` 是索引
注册表的键),单点 verb 有 flag 有 env,**缺的是 registry 一级,而且它们没有任何参数能命名设备**。

按 §12.2:第一步问「这是 core 一格通用能力缺失吗」——**是**。注册表里存着每台 sim 的
`runnerPort`(`smix sim register jp --udid … --runner-port 22088` 就是为此存在的),
而单点 verb 够不着它,于是在一台注册在非默认端口上的 sim 上,`smix tap` 会去拨 22087 而
`smix run` 拨 22088 —— **同一个工作区里两条命令拨不同的端口**。把文档改成「单点 verb 只有两级」
是把能力缺口写进契约,§13 拒。

### 口径 2 — `--device` 在单点 verb 上只做一件事:查端口

`smix run --device` 做两件事:选要驱动的 sim(udid 进 `run_flow`),以及查注册表拿端口。
单点 verb **只做后者** —— runner 是一个已经跑在某个端口上的进程,拨哪个端口就是拨哪台设备的
runner,没有第二重含义。

因此:
- 参数名与 `smix run` 一致(`--device`),取值同样是 **UDID 或注册表里的 alias / deviceName**
- 帮助文本必须写明它在这里的窄含义(「查这台设备注册的 runner 端口」),否则读者会以为它能切换目标
- **不**给单点 verb 加 `--platform` / `--udid` 等其它 `smix run` 的参数 —— 那是范围漂移

### 口径 3 — 覆盖面由 clap 树导出,不由手写清单定

「哪些子命令要加」由 C1 那条 probe 已经在用的规则决定:**凡有 `port` 参数的子命令**。
执行期不得凭记忆列名单 —— 列漏一个,闸门下一次运行就会点名它。

---

## 步骤(线性,2 个)

### S1. 让缺陷的反面先有断言

**红(写测试)**

- 文件:`crates/smix-cli/src/guide_gate.rs`
- 把 `the_registry_rung_is_still_unreachable_from_single_shot_verbs` **改写成正向形态**,
  改名 `every_runner_dialling_command_can_reach_the_registry`:
  - 遍历 `crate::Cli::command()`,凡有 `port` 参数的子命令,断言**同时**有 `device` 参数
  - 保留 `checked >= 8` 的反空转下界(C1 实测这类子命令不止 8 个)
  - 保留 `smix run` 的对照断言(`--runner-port` 带 env、`--device` 存在)——
    它是这条链「四级」说法的另一半依据,不能因为改了方向就丢
- 跑:`cargo test -p smix-cli --bin smix guide_gate`,应看到**红**,失败文本点名全部缺 `device` 的子命令

**绿(实现)**

- 文件:`crates/smix-cli/src/main.rs`
- 对每个有 `port` 的子命令加:

  ```rust
  /// Device UDID or a registry alias. Used here only to look up the
  /// runner port that device is registered on; it does not change
  /// which app or simulator the call is dispatched to.
  #[arg(long)]
  device: Option<String>,
  ```

- 端口解析从 `port.unwrap_or_else(act::runner_port_from_env)` 改为走同一条链:

  ```rust
  let p = run_port(port.or_else(|| act::runner_port_from_env_opt()), || {
      device.as_deref().and_then(lookup_registered).and_then(|s| s.runner_port)
  });
  ```

  **`runner_port_from_env` 要拆出一个不带默认值的版本**(`..._opt() -> Option<u16>`),
  否则 env 未设时它返回常量 22087,注册表那一级永远轮不到 —— 这正是 `run_port` 的
  `flag.or_else(registered).unwrap_or(22087)` 形状要求的。原函数保留(它是公开项且有单测),
  用新函数实现它。
- 文件:`crates/smix-cli/src/act.rs` —— 加 `runner_port_from_env_opt`,并把
  `runner_port_from_env` 改写成 `runner_port_from_env_opt().unwrap_or(DEFAULT_RUNNER_PORT)`。
  两个既有单测(`runner_port_from_env_default_when_unset` / 另一条)必须保持绿
- 跑:`cargo test -p smix-cli`,S1 那条转绿

**重构**

- 无。**不**把 9 个子命令的 `port` / `device` 抽成 `#[command(flatten)]` 的公共结构体:
  那会改动它们的 clap 表面顺序与帮助分组,而 `documented_flags_exist` 与 `05-cli.md` 都盯着
  这层表面。抽公共结构体是独立的一次改动,不与本段混。

### S2. 让表和文档都说真话,并证明它会红

**红(写测试)**

- 文件:`docs/guide-executability.md`
- N1 行改:`status` → `runs`,`probe` → `every_runner_dialling_command_can_reach_the_registry`,
  `层` → `—`,`依据` 换成**修复代码**的引用(不对称引文:`runs` 行钉修复,revert 即红),
  `复核` → 当天
- 跑:`cargo test -p smix-cli --bin smix guide_gate::the_list`,应绿(probe 名存在、`runs` 行 `层` 为 `—`)
- **装回缺陷验红**:临时从某一个子命令上摘掉 `device` 参数,重跑闸门,必须变红并点名那个子命令;
  恢复后重新变绿。这一步的结果写进决策日志

**绿(实现)**

- 文件:`docs/ai-guide/05-cli.md`
  - §Environment-variable precedence 保持不变(它现在是真的了)
  - 在单点 verb 那一节补一句 `--device` 的窄含义,与口径 2 的帮助文本同源
- 文件:`docs/v2.md` 决策日志追加一行,按 §10 格式,写明:
  - 为什么是改实现不是改文档(口径 1)
  - `runner_port_from_env` 拆 `_opt` 的理由(带默认值的函数吃掉了注册表那一级)
  - 装回缺陷的验红结果
- 跑:`bash scripts/dev/preflight.sh`

**重构**

- 无。

---

## Checkpoint C2 验收

```bash
cargo test -p smix-cli --bin smix guide_gate -- --nocapture 2>&1 | grep -E 'guide-executability:|test result:'
grep -c '| N1 |.*| runs |' docs/guide-executability.md
bash scripts/dev/preflight.sh
```

期望:

1. 摘要行为 `guide-executability: 8 claims (3 runs / 5 broken / 0 unjudged) · … 69 yaml blocks judged`;
   且 `test result: ok. … 0 failed`
2. 第二条输出 `1`
3. 第三条最后一行 `preflight: clean`

## 完成后动作

1. `mv docs/plan-hot.md docs/plan-history/v2.4-c2-hot.md`
2. 生成新 `docs/plan-hot.md`(覆盖 C3:`launchApp` 的 Activity 约定 = N2),附加本段专属 context:
   - 必读 `docs/guide-executability.md` 的 N2 行 —— 它的 probe 是**行为式**的
     (跑一条 `activity: .NotMainActivity` 的流,断言那个字符串没到达任何设备调用),
     不是「Kotlin 源里有没有这个字面量」的文本式判据
   - N2 牵动 `apps_config.rs`(解析已经收下 `activity`,只是无人读)与
     `RunnerWire.kt:157`(`am start -n $bundleId/.MainActivity` 钉死),**跨语言**
   - Kotlin 侧改动需要 `assembleDebugAndroidTest` 与 emulator;起设备前查用户活动 build
     (memory: `runner_ops_check_batch_owner_first`;Android 侧无 iOS 那样的显式-UDID 护栏,
     `gradlew install*` 会装到所有连着的设备,必须先 `export ANDROID_SERIAL=`)
