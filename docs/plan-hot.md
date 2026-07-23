# plan-hot — v2.9 到 C4：跨 triple `.node` prebuild matrix + CI + npm 分发机制 + smix-rn 接真 `@goliapkg/smix-node` 硬依赖/真工厂

## 目标 checkpoint

C4：**`crates/smix-node` 声明 `napi.targets = {aarch64-apple-darwin, x86_64-apple-darwin, x86_64-unknown-linux-gnu}`（win32 out-of-scope，决策日志），`napi create-npm-dirs` 在本机产出 3 个结构正确的 per-platform 子包目录（`@goliapkg/smix-node-{darwin-arm64,darwin-x64,linux-x64-gnu}`，各带正确 `os`/`cpu`/`libc`），`napi artifacts` 把本机 darwin-arm64 `.node` 落进其目录；CI 加一个 `napi-prebuild` matrix job，3 个 triple 各在原生 runner 构建 `.node` + 上传 artifact + 跑 `napi create-npm-dirs` 打包结构校验（`actionlint` 绿、matrix triple 集 == `napi.targets`）。同时新建根 `package.json` bun workspace 链 `crates/smix-node` ↔ `npm/smix-rn`，`npm/smix-rn` 加 `optionalDependencies: {"@goliapkg/smix-node": "workspace:*"}` + 真工厂 `loadNodeDriver(port?)`（动态 import 真 `SmixNodeDriver`，结构式满足 C3 的 `NodeDriver` seam），`Smix.launchApp` 增默认 driver 自动加载路径。本机 checkpoint 证：本 triple prebuild `.node` 存在 + 3 子包结构正确 + smix-rn 经真 `.node`（darwin-arm64）驱动一个 `node:http` loopback fake wire（复用 C2/C3 loopback 手法但走真 addon）+ smix-rn typecheck/vitest 全绿 + smix-node C1/C2 五套件不回归 + route-conformance rc=0 + `actionlint` 绿。** 跨 triple 实际产物（darwin-x64 / linux-x64）由 CI matrix 产，本机（darwin-arm64、无 linux）只核实 CI 配置正确、不产。**全程零 publish**（`napi`/`cargo`/`npm` publish 全不跑；ship.sh 的 smix-node 发布顺序 = 顺延，非本段）。不碰真设备（= C5）。

## 前置条件

```bash
test -f docs/plan-history/v2.9-c3-hot.md                                   # C3 热计划已归档
grep -q 'openSession' crates/smix-node/index.d.ts                         # 真 napi 面已在（工厂要包的真身）
grep -q 'export interface NodeDriver' npm/smix-rn/src/NodeDriver.ts       # C3 的 seam 在（真 addon 要满足的 interface）
grep -q "options?" npm/smix-rn/src/Smix.ts || grep -q 'driver: NodeDriver' npm/smix-rn/src/Smix.ts  # launchApp 现签名（本段改的对象）
test ! -f package.json                                                    # 根 workspace 尚不存在（本段从零建）
python3 scripts/dev/route-conformance.py                                  # 基线 rc=0（终端直读退出码，非管道）
command -v actionlint >/dev/null && echo "actionlint present"            # CI 配置校验器在（/opt/homebrew/bin/actionlint）
node --version                                                            # 期望 v26.x（napi 宿主）
```

## 已经查清、不必重查的事实

- **跨 triple 构建策略已定（决策日志 2026-07-24）= (a) CI matrix 原生 runner**，triple 集 = `{aarch64-apple-darwin, x86_64-apple-darwin, x86_64-unknown-linux-gnu}`。分工：`aarch64-apple-darwin` 在 `macos-14`（Apple Silicon）原生；`x86_64-apple-darwin` **同一 macos runner** 经 `rustup target add x86_64-apple-darwin` + `napi build --target x86_64-apple-darwin`（Apple 工具链两 SDK 齐备 = 可靠交叉）；`x86_64-unknown-linux-gnu` 在 `ubuntu-latest` 原生。拒 (b) 从 macOS 交叉编译 linux 原生 `.node`（需 zig/docker、脆弱、footgun）；拒 (c) 本地多机（非可复现发布路径）。**win32-x64 = out-of-scope**（RN/Expo 面 mac/linux 为主；index.js 加载垫片已含 win32 分支，未来纳入 = matrix 加一条 + 子包，零代码改）。

- **napi-rs v3 prebuild/分发范式（WebFetch 核实 napi.rs/docs：napi-config / create-npm-dirs / pre-publish）**：
  - `package.json` 的 `napi.targets` 是**要打包发布的 triple 数组**（string[]，接受 Rust triple 名如 `x86_64-unknown-linux-gnu`）。**声明 targets 不触发多 target 构建** —— `napi build` 每次只编译一个 target（host，或 `--target <triple>` 指定的那个）。故本机 `napi build --platform --release` 仍只产 darwin-arm64 `.node`；其余 triple 靠 CI matrix 的 `--target`。
  - `napi create-npm-dirs` 按 `napi.targets` **每 target 生成一个 `npm/<short>/` 子包目录**（`<short>` = 平台短名，如 `darwin-arm64`/`darwin-x64`/`linux-x64-gnu`；子包 npm 名 = `<packageName>-<short>` = `@goliapkg/smix-node-darwin-arm64` 等，与现有 `index.js` 加载垫片里 `require('@goliapkg/smix-node-<short>')` 的名字一致），各带 `os`/`cpu`（linux 另带 `libc`）字段。**这些 `npm/` 子包目录是构建/发布期生成物，不入库**（napi 官方模板亦不 check in `npm/`；本仓 `.gitignore` 同源忽略生成的 `index.js`/`.node`）。
  - `napi artifacts` 把已构建的 `.node` **映射进对应 `npm/<short>/` 目录**（本机只有 darwin-arm64 `.node`，故只填 darwin-arm64 子包；其余由 CI runner 各自填）。
  - `napi prepublish`（本段**不跑真跑**）= 把每 target 的 exact-version 子包合进主包 `optionalDependencies` + 逐个 `<npmClient> publish`。**发布顺延**，故本段既不 `prepublish` 真跑、也不给 ship.sh 塞未测 publish DAG（见「完成后动作」）。打包结构校验用 `create-npm-dirs`（无副作用、纯生成）即可，不需 `prepublish`。
  - 现有 `crates/smix-node/package.json` 已有 `"napi": {"binaryName": "smix-node"}` + `@napi-rs/cli ^3`；本段只补 `"targets": [...]`。`crates/smix-node/index.js`（napi 自动生成的加载垫片）**已含全 triple 分支**（darwin-arm64/darwin-x64/linux-x64-gnu/…，各先试本地 `./smix-node.<short>.node` 再试 `@goliapkg/smix-node-<short>` 子包）——**运行时选对 `.node` 的逻辑已就绪，本段零改 index.js**。

- **真 `SmixNodeDriver`/`SmixNodeSession` 逐字满足 C3 的 `NodeDriver`/`NodeSession` seam（`crates/smix-node/index.d.ts` vs `npm/smix-rn/src/NodeDriver.ts` 逐一对齐核实，无缺口）**：
  - `SmixNodeDriver`：`constructor(port: number)` / `tapById(id): Promise<boolean>` / `inputText(text): Promise<void>` / `pressKey(key): Promise<void>` / `swipe(direction): Promise<void>` / `tapAtCoord(nx,ny): Promise<string>` / `snapshotTree(): Promise<string>` / `systemPopups(): Promise<string>` / `openSession(bundleId): Promise<SmixNodeSession>` —— 与 `interface NodeDriver` 八方法签名逐一相符。
  - `SmixNodeSession`：`launchApp(): Promise<void>` / `terminateApp(): Promise<void>` / `relaunchApp(): Promise<void>` —— 与 `interface NodeSession` 三方法逐一相符。`SmixNodeDriver.openSession` 返回真 `SmixNodeSession`，**结构式满足** `NodeSession`。
  - 故真工厂 `new SmixNodeDriver(port)` 结构式满足 `NodeDriver`，生产直接落进 seam；C3 已按此形留缝（`NodeDriver.ts` 顶注即写「real driver is the napi addon…satisfy these interfaces structurally」）。**无 index.d.ts 与 seam 不符之处**。

- **smix-rn 接真硬依赖方式已定（决策日志 2026-07-24）= `optionalDependencies` + bun workspace**：
  - `optionalDependencies`（非 `dependencies`/`peer`）——非预构建平台 `install` 不破（可选依赖缺失被容忍）；真正的加载失败只在**运行时 `loadNodeDriver` 惰性 `import('@goliapkg/smix-node')`** 对不支持平台清晰抛出（这也是 index.js 加载垫片「所有 triple 都 miss → 抛 Cannot find native binding」的既有行为，非新防御）。
  - 本地解析（smix-node 未发布 + `private:true`）走**新建根 `package.json` bun workspace**：`{"name":"smix-workspace","private":true,"workspaces":["crates/smix-node","npm/smix-rn"]}`。smix-rn 依赖声明 `"@goliapkg/smix-node": "workspace:*"`（`bun publish` 时替换为 `2.0.0`，发布安全；本地 `bun install` 真符号链接到已构建 addon —— 比「声明 2.0.0 + 别名指源」更诚实，`bun install --frozen-lockfile` 真绿、真 `.node` 真加载）。
  - **lockfile 收敛**：现 `crates/smix-node/bun.lock` + `npm/smix-rn/bun.lock` 两把 → workspace 后收敛到**根 `bun.lock`**（§8.7 仍只 bun.lock，仅位置上移）。删两个子 lock、加根 lock。CI `ts-sdk` job 与 ship.sh 的 `cd npm/smix-rn && bun install` 在 workspace 下仍工作（bun 上溯至 workspace 根用根 lock）；`--frozen-lockfile` 指向根 lock —— S3 相应把 CI `ts-sdk` 的 install 调到 workspace 根（本机以 `bun install --frozen-lockfile` 从根跑通作代理验证）。

- **真工厂 `loadNodeDriver(port?)` 形（`npm/smix-rn/src/loadNodeDriver.ts` 新建）**：`export async function loadNodeDriver(port?: number): Promise<NodeDriver>` = `const { SmixNodeDriver } = await import('@goliapkg/smix-node'); return new SmixNodeDriver(port ?? defaultRunnerPort())`。默认端口 `defaultRunnerPort()` = `Number(process.env.SMIX_RUNNER_PORT) || 22087`（**22087 = `crates/smix-cli/src/act.rs:35` 的 `DEFAULT_RUNNER_PORT`,本机核实**；env 覆盖名 `SMIX_RUNNER_PORT` 与 CLI `runner_port_from_env` 同源）。typecheck 解析 `@goliapkg/smix-node` 类型经 workspace symlink 的 `index.d.ts`（真面）。**动态 `import()` 是惰性的**——非预构建平台不在 install 期炸，只在真调 `loadNodeDriver` 时抛，正确。

- **`Smix.launchApp` 签名改（决策日志 2026-07-24）**：C3 现签名 `launchApp(target, driver: NodeDriver, resolver, labelsResolver?)`（`Smix.ts:24`，driver 必填、显式传）。改为 **`launchApp(target, resolver, options?: { driver?: NodeDriver; labelsResolver?: LabelsResolver })`**：`const driver = options?.driver ?? await loadNodeDriver();` 其余不变（`openSession`→`launchApp`→`new App(...)`）。这**字面满足**「launchApp 有个默认 driver 路径」——省略 `options.driver` = 生产黄金路径自动载真 addon，传 `{ driver: mock }` = DI 保留可测。
  - **blast radius（本机 grep 核实）= smix-rn 自家 2 处 launchApp 测试 caller**：`__tests__/AppDriving.test.ts:101`（`Smix.launchApp(bundleId('com.acme.app'), driver, resolver.resolve)`）、`__tests__/MvpApiShape.test.ts:112`。两处改为新形 `Smix.launchApp(bundleId('…'), resolver.resolve, { driver })`。本段内一并更新。**`new App(` 3 处 caller（AppDriving:13/MvpApiShape:108/ReadmeSnippets:101）不受影响**——App 构造签名 C4 不改（工厂只动 launchApp 的默认 driver 路径）。
  - **README 措辞校准**：`npm/smix-rn/README.md:6-13` 现写「Live driving now runs through the napi addon…The bundled per-platform addon…lands with the prebuild matrix **in a later release**; until then, **inject a `NodeDriver`**」。C4 后 prebuild matrix 已 wire（CI 产 artifact）但**未发布**（顺延）——README 改为诚实态：per-platform addon 由 prebuild matrix 在 CI 产出、`loadNodeDriver` 自动加载已安装的 addon，**发布随后续 release**；仍可显式注入 `NodeDriver`（测试/自定义）。**不谎称已发布**（`honesty/no-false-verified` 同源）。ReadmeSnippets.test.ts 无 launchApp 位置形 caller（:46 是 `session.relaunchApp()`、:101 是 `new App`），故 README 改动不牵动 ReadmeSnippets 的 throw-断言；但改 README 散文后仍需 `bun run test`（ReadmeSnippets 正则提取方法名）绿。

- **CI 现状（`.github/workflows/ci.yml` 核实）**：单文件，job = `rust-and-swift`(macos-15) / `ts-sdk`(ubuntu,bun typecheck+vitest) / `android-no-device` / `source-gates`(route-conformance 等在此) / `server-integration`。**无 napi/prebuild job**。本段加 `napi-prebuild` matrix job（3 triple）。`actionlint` 在本机（`/opt/homebrew/bin/actionlint`）= CI 配置机器可判校验器（PyYAML 不可用，故用 actionlint 而非自写 yaml 解析）。

- **真 addon 过 loopback 的测试形（复用 C2/C3 loopback 手法，但走真 `.node`；vitest 而非 node:test）**：smix-rn 用 vitest（`"test":"vitest run"`）。测试用 Node 内置 `node:http` 起本地 server（`server.listen(0)` 取随机 loopback 端口），按 `req.method+req.url` 回预置 wire JSON（GET `/tree` 回 `{"rawType":"application","identifier":"root"}`、POST `/tap-by-id` 回 `{"ok":true}` 等，字段形以 `smix-runner-wire` 为准，同 C2 `__test__/*.mjs`）；`const driver = await loadNodeDriver(port)` → 真 `SmixNodeDriver` 指向 loopback → `Smix.launchApp(bundleId('x'), new MockSelectorResolver().resolve, { driver })`（resolver 仍是纯 seam，注 mock；driver 是真 addon）→ `app.snapshotTree()` / `app.tap(Selector.id('root'))` 经**真 `.node`** 打 loopback，Promise 真 resolve。这证「smix-rn 经真 addon 驱动 loopback fake wire」，无真设备、无 publish。native addon 在 vitest 的 Node worker 里加载正常。

- **零 publish 边界（决策日志 + 任务约束 2）**：本段搭**分发机制**（`napi.targets` + `create-npm-dirs` 子包结构 + `artifacts` 落 `.node` + 已就绪的 index.js 加载垫片 + smix-rn optionalDependencies 硬依赖 + 真工厂 + CI matrix 产 artifact）。**不跑任何 publish**：`napi prepublish` 不真跑（只 create-npm-dirs 校验结构）、`cargo publish`/`npm publish` 不跑、ship.sh 的 smix-node 发布顺序留顺延。checkpoint 命令里**无一条 publish**。

## 步骤（线性，3 个，按面分组）

### S1. smix-node prebuild 打包机制：`napi.targets` + `create-npm-dirs` 3 子包结构

**红（写测试，先失败一次）**
- 文件：`crates/smix-node/__test__/npm-dirs.test.mjs`（新建，`node:test` + `node:assert` + `node:fs`）。
- 断言（先 `bun run build` 产本机 `.node`，再 `bunx napi create-npm-dirs` 生成 `npm/`，然后读断言）：
  1. `crates/smix-node/npm/` 下**恰好 3 个**目录：`darwin-arm64`、`darwin-x64`、`linux-x64-gnu`（排序后相等）。
  2. 各子包 `npm/<short>/package.json`：`name === '@goliapkg/smix-node-<short>'`；`os`/`cpu` 正确（`darwin-arm64`→os `darwin`/cpu `arm64`；`darwin-x64`→os `darwin`/cpu `x64`；`linux-x64-gnu`→os `linux`/cpu `x64`/`libc` 含 `glibc`）。
  3. `bunx napi artifacts` 后 `npm/darwin-arm64/` 下存在 `smix-node.darwin-arm64.node`（本机 triple 的 `.node` 被映射进去）。
- 跑：`cd crates/smix-node && node --test __test__/npm-dirs.test.mjs` → 期望**红**（`napi.targets` 未声明，`create-npm-dirs` 不产 3 子包 / 产不出预期集合）。

**绿（实现，最少改动转绿）**
- 文件：`crates/smix-node/package.json`：`napi` 字段补 `"targets": ["aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-unknown-linux-gnu"]`（`binaryName` 保留）。
- 文件：`crates/smix-node/.gitignore`：加 `/npm/`（`create-npm-dirs` 生成物不入库，同现有忽略 `index.js`/`.node` 的理由；注释一行 WHY = build/publish-time artifact）。
- 关键点：① `napi.targets` 只声明发布 triple 集，不触发多 target 构建（本机仍只产 darwin-arm64 `.node`）；② `create-npm-dirs`/`artifacts` 纯生成/映射，无 publish 副作用；③ index.js 加载垫片已就绪，不改。
- 跑：`cd crates/smix-node && bun run build && bunx napi create-npm-dirs && bunx napi artifacts && node --test __test__/npm-dirs.test.mjs` → 期望**绿**。

**重构（可选）**
- 无。

### S2. 根 bun workspace + smix-rn 硬依赖/真工厂 + `Smix.launchApp` 默认 driver + 真 addon 过 loopback

**红（写测试，先失败一次）**
- 文件：`npm/smix-rn/src/__tests__/RealAddonLoopback.test.ts`（新建，vitest + `node:http`）。
- 断言：
  1. `const driver = await loadNodeDriver(port)`（真 `@goliapkg/smix-node` 经 workspace 解析）返回对象，其 `snapshotTree`/`tapById`/`openSession` 皆为函数（结构式满足 `NodeDriver`）。
  2. loopback server 应答 GET `/tree` 回 `{"rawType":"application","identifier":"root"}`；`await driver.snapshotTree()` 经**真 `.node`** resolve，`JSON.parse(...).identifier === 'root'`。
  3. `const app = await Smix.launchApp(bundleId('com.acme.app'), new MockSelectorResolver().resolve, { driver })`；loopback 应答 `/session/open`+`/session/launch-app`，`app` 是 `App` 实例（launchApp 走 `{ driver }` 显式路径）。
  4. resolver 对 `Selector.id('root')` 注册命中 `['root']`；loopback 应答 POST `/tap-by-id` 回 `{"ok":true}`；`await app.tap(Selector.id('root'))` 经真 `.node`（snapshotTree→resolve→tapById）不抛、loopback 收到 `/tap-by-id` body `.id === 'root'`。
- 跑：`cd npm/smix-rn && bun run test src/__tests__/RealAddonLoopback.test.ts` → 期望**红**（`loadNodeDriver` 不存在、`@goliapkg/smix-node` 未解析、launchApp 旧签名不吃 options）。

**绿（实现，最少代码转绿）**
- 新文件根 `package.json`：`{"name":"smix-workspace","private":true,"workspaces":["crates/smix-node","npm/smix-rn"]}`。删 `crates/smix-node/bun.lock` + `npm/smix-rn/bun.lock`，在根 `bun install` 生成根 `bun.lock`。
- 文件：`npm/smix-rn/package.json`：加 `"optionalDependencies": { "@goliapkg/smix-node": "workspace:*" }`。
- 新文件：`npm/smix-rn/src/loadNodeDriver.ts`：`export async function loadNodeDriver(port?: number): Promise<NodeDriver>` = 动态 `import('@goliapkg/smix-node')` 取 `SmixNodeDriver`、`new SmixNodeDriver(port ?? defaultRunnerPort())`（`defaultRunnerPort` = `Number(process.env.SMIX_RUNNER_PORT) || 22087`）。`import type { NodeDriver } from './NodeDriver.js'`。
- 文件：`npm/smix-rn/src/Smix.ts`：`launchApp` 改 `(target, resolver, options?: { driver?: NodeDriver; labelsResolver?: LabelsResolver })`；`const driver = options?.driver ?? await loadNodeDriver();` `appPath` 分支不变（host-侧-安装 throw）；`openSession`→`launchApp`→`new App(target.value, driver, session, resolver, options?.labelsResolver)`。`import { loadNodeDriver } from './loadNodeDriver.js'`。
- 文件：`npm/smix-rn/src/index.ts`：导出 `loadNodeDriver`。
- 文件：更新 2 处 launchApp 测试 caller（`AppDriving.test.ts:101`、`MvpApiShape.test.ts:112`）为新形 `Smix.launchApp(target, resolver, { driver })`。
- 文件：`npm/smix-rn/README.md`（:6-13）校准措辞为诚实态（addon 由 prebuild matrix 在 CI 产出、`loadNodeDriver` 自动加载已安装 addon、发布随后续 release、仍可显式注入 `NodeDriver`）——不谎称已发布。
- 关键点：① `optionalDependencies` + 惰性动态 import = 非预构建平台 install/import 安全；② workspace `workspace:*` 本地真符号链接、发布时替换版本；③ launchApp options-object 字面满足「默认 driver 路径」+ DI 保留。
- 跑：`cd /Users/doracawl/workspace/goliajp/smix && bun install && ( cd npm/smix-rn && bun run test src/__tests__/RealAddonLoopback.test.ts )` → 期望**绿**。

**重构（可选）**
- 若 loopback server 装配与 C2 `__test__` 手法重复，抽一个测试内 `startFakeWire(routes)` helper；不改断言。

### S3. CI `napi-prebuild` matrix + workspace install 收口 + `actionlint`

**红（写测试，先失败一次）**
- 文件：`crates/smix-node/__test__/ci-config.test.mjs`（新建，`node:test` + `node:fs`；纯文本/结构校验，不跑 CI）。
- 断言：
  1. `actionlint` 对 `.github/workflows/ci.yml` 退出 0（`child_process.execFileSync('actionlint', ['.github/workflows/ci.yml'])` 不抛）。
  2. `ci.yml` 含 `napi-prebuild` job，且其 matrix 覆盖恰好 3 个 triple 字符串（`aarch64-apple-darwin`/`x86_64-apple-darwin`/`x86_64-unknown-linux-gnu`），与 `crates/smix-node/package.json` 的 `napi.targets` 集合相等（读两处比对，非硬编码单侧）。
  3. `napi-prebuild` job **不含** `napi prepublish`/`npm publish`/`cargo publish` 等 publish 动词（grep job 段无 publish 字面量；分发机制 job 只 build+create-npm-dirs+upload-artifact）。
- 跑：`cd crates/smix-node && node --test __test__/ci-config.test.mjs` → 期望**红**（无 `napi-prebuild` job）。

**绿（实现）**
- 文件：`.github/workflows/ci.yml`：加 `napi-prebuild` job：
  - `strategy.matrix.settings` 3 条：`{ target: aarch64-apple-darwin, host: macos-14 }` / `{ target: x86_64-apple-darwin, host: macos-14 }` / `{ target: x86_64-unknown-linux-gnu, host: ubuntu-latest }`；`runs-on: ${{ matrix.settings.host }}`。
  - steps：checkout → `Swatinem/rust-cache` → `oven-sh/setup-bun` → `rustup target add ${{ matrix.settings.target }}` → `bun install --frozen-lockfile`（workspace 根）→ `cd crates/smix-node && bunx napi build --platform --release --target ${{ matrix.settings.target }}` → `bunx napi create-npm-dirs`（打包结构校验，不 publish）→ `actions/upload-artifact` 传 `crates/smix-node/*.node`。
  - **无 publish step**（真发布顺延）。
- 文件：`.github/workflows/ci.yml` 的 `ts-sdk` job：install 调到 workspace 根（`working-directory` 去掉 `npm/smix-rn` 只在 install 步、或 install 步在根跑 `bun install --frozen-lockfile` 后 typecheck/test 仍 `working-directory: npm/smix-rn`）——使根 `bun.lock` 是 frozen 源。
- 关键点：① 每 triple 原生 runner（darwin-x64 经 macos-14 `--target`，linux 经 ubuntu 原生）；② 只 build + create-npm-dirs + upload artifact，零 publish；③ triple 集单一真源 = `napi.targets`，CI matrix 由 ci-config 测试钉住与之相等。
- 跑：`cd crates/smix-node && node --test __test__/ci-config.test.mjs` → 期望**绿**。

**重构（可选）**
- 无。

## Checkpoint C4 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix \
  && python3 scripts/dev/route-conformance.py \
  && bun install --frozen-lockfile \
  && ( cd crates/smix-node && bun run build \
        && bunx napi create-npm-dirs && bunx napi artifacts \
        && node --test __test__/load.test.mjs __test__/tap.test.mjs \
             __test__/act.test.mjs __test__/sense.test.mjs __test__/session.test.mjs \
             __test__/npm-dirs.test.mjs __test__/ci-config.test.mjs ) \
  && ( cd npm/smix-rn && bun run typecheck && bun run test ) \
  && actionlint .github/workflows/ci.yml \
  && echo C4-PASS
```

期望：stdout 末尾打印 `C4-PASS`，exit 0。含义（`&&` 链任一环非零即中止、无 `C4-PASS`）：
1. **`route-conformance.py` 退出码由终端直读**（链首环，rc=0：workspace/硬依赖/工厂改动未引入新增待服务路由，parity 基线守住）。
2. **`bun install --frozen-lockfile`（workspace 根）绿** —— 根 bun.lock 与 workspace（含 `@goliapkg/smix-node` symlink）一致、可复现（= CI `--frozen-lockfile` 将做的事）。
3. `crates/smix-node`：`.node` 重建 + `create-npm-dirs`/`artifacts` 无副作用生成 + **C1/C2 五套件不回归** + `npm-dirs`（3 子包结构 + os/cpu/libc + darwin-arm64 `.node` 落位）+ `ci-config`（actionlint 绿 + matrix triple 集 == `napi.targets` + job 无 publish 动词）全过。
4. `npm/smix-rn`：**typecheck 绿**（`@goliapkg/smix-node` 类型经 workspace 解析、`loadNodeDriver`/launchApp options-object 无类型洞）+ **vitest 全绿**（含 `RealAddonLoopback` 经真 `.node` 驱动 loopback、既有 AppDriving/MvpApiShape/ReadmeSnippets/SelectorFullSchema 在 launchApp 新签名下更新后绿）。
5. `actionlint` 对 ci.yml 绿（CI 配置结构正确；跨 triple 实际产物由 CI matrix 产，本机不产、只核实配置）。
6. **零 publish** —— 上链无一条 `napi prepublish`/`cargo publish`/`npm publish`；只 build + create-npm-dirs + artifacts（纯生成/映射）+ 各 gate。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.9-c4-hot.md`。
2. 跨 triple 构建策略（CI matrix）+ win32 out-of-scope + smix-rn `optionalDependencies`/workspace/工厂/launchApp 决策已在本段执行前写入 `docs/v2.md` 决策日志（2026-07-24 两行）；无需重复。
3. **ship.sh 的 smix-node prebuild+prepublish 发布顺序 = 顺延项**（发布不可撤销，随 v2.9–v2.12 全完 + 用户显式授权时接线；届时在 `scripts/release/ship.sh` 的 `# --- publish npm ---` 段前加：下载 CI matrix artifacts → `napi artifacts` → `napi prepublish`（发主包 + 3 子包）→ 再 `bun publish @goliapkg/smix`。C4 不写未测 publish DAG、不改 ship.sh）。
4. 调 sub-agent 热化 **C5**（TS 驱动设备 e2e + 四 SDK parity 闭合 —— TS 对真 sim 跑一条 flow 绿，与 Swift/Kotlin driving parity；两个 "Session" 概念在真设备上的会话协调（C3 记的 finding）属 C5 范畴），见 CLAUDE.md §6。C5 前须核实：C4 的 prebuild matrix 在 CI 三 triple 皆产出 artifact（一次真 CI run 的产物核对）、`loadNodeDriver` 对真 runner 端口（22087）连通。
