# plan-hot — v2.9 到 C1：napi 脚手架 + 单方法 tapAtCoord 端到端（darwin-arm64，fake wire，async 桥验通）

## 目标 checkpoint

C1：**新 `crates/smix-node` crate 存在，napi-rs 产出 darwin-arm64 `.node`，Node 经该 `.node` 对一个 loopback 上的 fake wire（`node:http` 本地 mock，不是真设备）真发一次 tap，拿到结构化结果（tap 落点链），且 tokio async 桥不 panic。** 这一步只证「一根经 napi 到达 `smix-runner-client` 的 async 桥能通、能带结构化结果回来」——不退 `App.ts` 13 桩（C3）、不暴露驱动全面（C2）、不做跨 triple 矩阵（C4）、不碰真 sim（C5）。

## 前置条件

```bash
git status --short | grep -q 'plan-history/v2.8-c7-hot.md'                 # 上一段热计划已归档
grep -q 'SmixNotImplementedError' npm/smix-rn/src/App.ts                    # 13 桩仍在（C1 不退，只搭桥）
grep -c 'pub fn resolve_selector' crates/smix-ffi/src/lib.rs                # 现 smix-ffi 只暴露 resolver stone（driving 在 driving.rs 走 uniffi export，非本段扩的对象）
grep -q 'pub async fn tap_at_norm_coord' crates/smix-runner-client/src/lib.rs # napi 要绑的真身 async 方法在
test -d crates/smix-node && echo "ERROR: smix-node 已存在，本段假设从零建" || true  # 期望不打印 ERROR
node --version   # 期望 v26.x（napi 宿主）
```

## 已经查清、不必重查的事实

- **架构决策已定（进 `docs/v2.md` 决策日志 2026-07-24）：走新 `crates/smix-node` crate，napi-rs 直绑 `smix-runner-client`，零触碰 `smix-ffi` UniFFI 面。** 依据（读源码核实）：
  - `smix-ffi` 是 UniFFI 专用 crate：`uniffi::include_scaffolding!("smix")` + `#[uniffi::export]` + `crate-type=["staticlib","cdylib","rlib"]`，产 Swift/Kotlin 绑定。napi-rs 用**另一套**构建模型（`@napi-rs/cli` 的 `napi build` 产 `.node` cdylib + napi 自己的 proc-macro），把两套绑定系统塞进同一 cdylib 会耦合构建图、并让每次动 napi 面都波及 Swift/Kotlin 消费者的 semver（回归风险）。
  - 现架构已是「一份 wire client，多绑定」：`smix-ffi/src/driving.rs` 的 `SmixDriver { client: Arc<HttpRunnerClient> }` 就是 `smix-runner-client` 的**薄封装**，经 `#[uniffi::export(async_runtime = "tokio")]` 暴露给 Swift/Kotlin。新 `smix-node` 用同样的薄封装模式包**同一个** `HttpRunnerClient`，只是绑定层换成 napi——正统、隔离、零回归。这是首选路，非分叉。
- **napi 要绑的真身**：`crates/smix-runner-client/src/lib.rs` 的 `HttpRunnerClient`——
  - 构造：`HttpRunnerClient::new(port: u16)`（→ `http://127.0.0.1:{port}`）与 `with_base<S: Into<String>>(base)`（`:549`/`:556`）。它是**纯 reqwest HTTP client**，不含任何设备代码。
  - tap 是 **async**：所有驱动方法都是 `pub async fn`（tokio）。C1 选绑的单方法 = `pub async fn tap_at_norm_coord(&self, nx: f64, ny: f64) -> Result<TapAtCoordResult, RunnerTransportError>`（`:1297`，`POST /tap-at-norm-coord`）。
  - 返回结构化：`TapAtCoordResult { chain: Vec<HitChainEntry> }`（`crates/smix-runner-wire/src/lib.rs:249`，derive `Serialize`）——命中点落进的元素链，即 C1 要带回 Node 的「结构化结果」。
- **为什么 C1 选 `tap_at_norm_coord` 而非 selector `tap`**：`tap(&Selector, TapMode, Option<IncludeScope>)`（`:1270`）需跨边界 marshal `Selector` + `TapMode` + session 语境。C1 只需**证桥通 + 带结构化结果**，`tap_at_norm_coord` 两个 `f64` 进、一个链出、**无 Selector marshaling、无 session**，是最小可证单方法。它也正对应 `App.tapAtCoord`（13 桩之一）。selector 版 `tap` + 驱动全面 = **C2**（不在本段）。
- **fake wire = loopback HTTP mock，不需真设备**：`HttpRunnerClient` 只往 `base + route` 发 HTTP。仓库既有先例——`crates/smix-ffi/tests/driving.rs` 用 `wiremock::MockServer` 在随机 loopback 端口 fake `/tree` 等路由，`SmixDriver::new(port_of(server))` 跑通往返，**零设备**。C1 在 Node 侧镜像同一手法：用 Node 内置 `node:http` 起一个本地 server 应答 `POST /tap-at-norm-coord`，napi 绑定指向它的端口。所以冷计划「C1 不需设备」成立：fake wire 就是本地 HTTP mock。
- **napi-rs async 桥范式（已 WebSearch 核实，napi-rs 官方文档 `napi.rs/docs/concepts/async-fn`）**：`@napi-rs/cli` 当前稳定 = **3.7.0**；`napi` / `napi-derive` = 3.x。napi-rs **默认自带 tokio runtime**——`#[napi]` 标注 `async fn`，await 一个 tokio/reqwest future 时 napi-rs 在其内置 tokio runtime 上驱动它，并把导出函数转成 JS `Promise`。这正是 `smix-ffi/src/driving.rs` 里 `#[uniffi::export(async_runtime = "tokio")]` 的 napi 对等物：**无需手动寄宿 runtime**。「async 桥验通」的机器判据 = Promise 真 resolve 出结构化结果（无 reactor 的 future 会 panic，正是 `driving.rs` 顶注警告的失败态）。
- **本机探测（真跑）**：`node v26.5.0` / `npm 12.0.1` / `cargo 1.97.1` / `rustc host: aarch64-apple-darwin`（= darwin-arm64，单 triple 满足）。`@napi-rs/cli` 未全局装（`command -v napi` 空 + `npx --no-install` 报 could not determine executable），故本段**要把它加进 `crates/smix-node` 的 devDependencies**（napi-rs 标准范式：Rust crate 目录同时是 npm 包）。
- **包管理器 = bun（§8.7）**：`npm/smix-rn` 用 `bun.lock`。新 crate 的 JS 侧同用 bun（`bun install` / `bun run build`）。运行期测试用 Node 内置 test runner（`node --test` + `node:test` + `node:http`，零额外 npm 依赖）——因为加载 `.node` addon 必须在真 Node runtime 里，这也正是本段要证的东西。

## 步骤（线性，2 个）

### S1. 立起 `smix-node` crate 并证 `.node` 能构建 + 能在 Node 加载

**红（写测试，先失败一次）**
- 文件：`crates/smix-node/__test__/load.test.mjs`
- 断言（用 `node:test` + `node:assert`）：`import('../index.js')` 成功；导出的 `SmixNodeDriver` 是构造函数；`new SmixNodeDriver(0)` 不抛（构造只建 `HttpRunnerClient`，不连任何东西）。
- 跑：`cd crates/smix-node && node --test __test__/load.test.mjs` → 期望**红**（`index.js` / `.node` 都不存在，import 失败）。这是「测在测真实产物」的证明。

**绿（实现，最少代码让红转绿）**
- 文件：`crates/smix-node/Cargo.toml`
  - `[package]` 用 `version.workspace=true` 等 workspace 继承；`publish = false`（同 smix-ffi 理由：二进制 artifact，不过 crates.io）。
  - `[lib] crate-type = ["cdylib"]`。
  - deps：`napi = "3"`、`napi-derive = "3"`、`smix-runner-client = { path = "../smix-runner-client", version = "2.0.0" }`、`serde_json = { workspace = true }`、`tokio = { workspace = true, features = ["rt-multi-thread","macros"] }`（napi 默认 runtime 需要）。
  - build-deps：`napi-build = "2"`。
- 文件：`crates/smix-node/build.rs` — `napi_build::setup();`。
- 文件：`crates/smix-node/package.json`
  - `name`（本地即可，如 `@goliapkg/smix-node`）、`"napi": { "binaryName": "smix-node" }`（令产物名确定 = `smix-node.darwin-arm64.node`）。
  - `devDependencies`: `"@napi-rs/cli": "^3"`。
  - `scripts`: `"build": "napi build --platform --release"`（同时生成 `index.js` + `index.d.ts` 的平台加载垫片）。
- 文件：`crates/smix-node/src/lib.rs`
  - API：`#[napi] pub struct SmixNodeDriver { client: std::sync::Arc<smix_runner_client::HttpRunnerClient> }`；`#[napi] impl SmixNodeDriver { #[napi(constructor)] pub fn new(port: u32) -> Self }`（`port as u16` 建 `HttpRunnerClient::new`）。本步**不加 tap 方法**——只证 crate 立起、`.node` 构建、Node 能加载并构造。
  - 关键点：`port: u32`（napi 无 u16 原生映射，边界收窄到 `as u16`）；`Arc` 持有 client 以便 S2 的 `&self` async 方法共享。
- 跑：`cd crates/smix-node && bun install && bun run build && node --test __test__/load.test.mjs` → 期望**绿**。

**重构（可选）**
- 仅整理 Cargo.toml 字段顺序 / 确认 `.gitignore` 忽略 `target` 与生成的 `*.node`（不改行为）。

### S2. 绑 `tapAtCoord` 过 tokio async 桥，证一次 tap 端到端穿 fake loopback wire

**红（写测试，先失败一次）**
- 文件：`crates/smix-node/__test__/tap.test.mjs`
- 断言（`node:test` + `node:http`）：
  1. 用 `node:http` 起本地 server（`server.listen(0)` 取随机 loopback 端口），对 `POST /tap-at-norm-coord` 回 `200` + body `{"chain":[{"identifier":"btn-ok"}]}`（HitChainEntry 的最小可反序列化形；如需补字段，以 `smix-runner-wire` 的 `HitChainEntry` 定义为准）。
  2. `const d = new SmixNodeDriver(port)`；`const raw = await d.tapAtCoord(0.5, 0.5)`。
  3. 断言 `await` 真 resolve（不 hang / 不 panic），`JSON.parse(raw).chain[0].identifier === 'btn-ok'`。
  4. 关闭 server。
- 跑：`cd crates/smix-node && node --test __test__/tap.test.mjs` → 期望**红**（`tapAtCoord` 尚未绑，方法不存在 / TypeError）。

**绿（实现）**
- 文件：`crates/smix-node/src/lib.rs` 的 `impl SmixNodeDriver` 加：
  - API：`#[napi] pub async fn tap_at_coord(&self, nx: f64, ny: f64) -> napi::Result<String>`（napi 默认把 snake_case 导出为 JS `tapAtCoord`）。
  - 体：`let r = self.client.tap_at_norm_coord(nx, ny).await.map_err(|e| napi::Error::from_reason(e.to_string()))?; Ok(serde_json::to_string(&r).map_err(|e| napi::Error::from_reason(e.to_string()))?)`。
  - 关键点：① 返回 JSON `String`——与 `smix-ffi/src/driving.rs` 里 `SmixDriver::tree` 返回 `String` 同源约定，C1 最小 marshaling；结构化性由 Node 侧 `JSON.parse().chain` 保证。② `RunnerTransportError` → `napi::Error::from_reason`（边界跨语言，是契约上报，非 §code 防御）。③ `&self` async + napi 默认 tokio runtime = reqwest future 在 napi 的 reactor 上被 poll，导出为 JS Promise——**这一步 resolve 即「async 桥验通」**。
- 跑：`cd crates/smix-node && bun run build && node --test __test__/tap.test.mjs` → 期望**绿**。

**重构（可选）**
- 若 `HitChainEntry` 真实字段多于测试所设，只补 mock body 字段使反序列化稳；不改 `tap_at_coord` 语义。

## Checkpoint C1 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix/crates/smix-node \
  && bun install \
  && bun run build \
  && ls ./*.node >/dev/null 2>&1 \
  && node --test __test__/load.test.mjs __test__/tap.test.mjs \
  && echo C1-PASS
```

期望：stdout 末尾打印 `C1-PASS`，exit 0。含义 = `crates/smix-node` 构建出 darwin-arm64 `.node`（`ls ./*.node` 命中）；两个 Node 测试皆过——(load) `.node` 能在真 Node runtime 加载 + `SmixNodeDriver` 可构造；(tap) 经 napi tokio async 桥对 loopback fake wire 发一次 `tapAtCoord`，Promise resolve 出结构化落点链（`chain[0].identifier`），**全程无真设备、无 panic**。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.9-c1-hot.md`。
2. 架构决策已在本段执行前写入 `docs/v2.md` 决策日志（2026-07-24 一行）；无需重复。
3. 调 sub-agent 热化 **C2**（`smix-runner-client` 驱动全面经 napi 边界暴露 —— tap/fill/swipe/screenshot/snapshotTree/… 纯逻辑/loopback 单测覆盖），见 CLAUDE.md §6。C2 才引入 selector 版 `tap` 的跨边界 marshaling 与 session 语境；本段刻意不碰。
