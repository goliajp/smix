# plan-hot — web v0.2 到 C2：4 主页真内容 + 27-tool build-time generator

> [3/4 信息层] **target = `web/` sub-project v0.2 C2**（不是 simx 主项目）。范围严格止于 4 主页 mdx + tools generator + home CTA。**不**做 examples sub-page（C3）、**不**激活 shiki runtime（C3）、**不**做 docs ↔ simx-repo 自动 sync 脚本（推 web v0.3+）。完成后归档到 `docs/plan-history/web-v0.2-c2-hot.md`。

## 目标 checkpoint

**web v0.2 C2**：4 docs 主页承载真内容 + tools 页 build-time 生成。世界变成：

1. `web/content/quick-start.mdx` 替换 C1 占位：内容来自 `README.md` §Quick start + §Why another one 浓缩 + 一段 "更多见 GitHub" pointer
2. `web/content/plugin-install.mdx` 新增：内容来自 `docs/plugin-install.md` 全文（手 copy 一次）
3. `web/content/authoring.mdx` 新增：内容来自 `README.md` §Authoring guide for AI agents 整段（line 134–245，手 copy 一次）
4. `web/content/tools.mdx` 新增：**build-time generator** 输出。27 个 `<h3>{tool.name}</h3>` + description + JSON schema fenced block；按 7 group 分段：ping / lifecycle / observe / interaction / compound / system / vlm
5. `web/scripts/generate-tools-page.ts` 新增：bun 脚本，`import("../../src/mcp/tools.ts").ALL_TOOLS` 读 27 spec，写 `web/content/tools.mdx`。**生成器失败 → 立即 fail build**（exit ≠ 0）
6. `web/package.json` `scripts.prebuild` 跑 generator；`scripts.build` 改为 `bun run prebuild && tsc --noEmit && prettier --write src/ && vite build`
7. `web/content/nav.config.ts` 从 1 项扩到 5 项（4 主页 + 1 examples 占位 disabled）：`quick-start` / `plugin-install` / `authoring` / `tools` / `examples`（examples 项的 mdx 在 C3 落，C2 仅 nav 占位 + slug 找不到时仍走 docs-local 404）
8. `web/src/views/home.tsx` `LINKS` 中三个 external link 全部改为 internal docs link（`/docs/quick-start` / `/docs/plugin-install` / `/docs/authoring`）；并在 hero 区下方加一个 "Read the docs" 突出 CTA Link 到 `/docs`

## 前置条件

```bash
# 仍在 web v0.2 feature 分支
test "$(git rev-parse --abbrev-ref HEAD)" = "feature/web-v0.2"

# C1 已 close
test -f docs/plan-history/web-v0.2-c1-hot.md
test -f web/content/quick-start.mdx
test -f web/content/nav.config.ts
test -f web/src/views/docs/page.tsx
test -f web/src/views/docs/routes.tsx

# C1 测试当前全绿
cd web && bun x vitest run src/views/docs/__tests__/docs-route.test.tsx 2>&1 | tail -3 | grep -q '3 passed'

# generator 必需的 SoT 文件存在
test -f src/mcp/tools.ts
grep -q 'export const ALL_TOOLS' src/mcp/tools.ts
grep -q '^## Authoring guide for AI agents' README.md
test -f docs/plugin-install.md && test "$(wc -l < docs/plugin-install.md)" -ge 60

# C2 目标文件还未存在
test ! -e web/content/tools.mdx
test ! -e web/content/plugin-install.mdx
test ! -e web/content/authoring.mdx
test ! -e web/scripts/generate-tools-page.ts
```

## 步骤（线性，无分叉；3 步）

### S1. 27-tool generator（TDD 严格红→绿）

**红（写测试）**
- 文件：`web/src/__tests__/tools-generator.test.ts`（新）
- 断言（**3 个 case**）：
  1. `await import('../../scripts/generate-tools-page.ts')` 解出 default export `generateToolsPage(): Promise<string>`；返回字符串首行 `---\ntitle: MCP tools reference\n---`
  2. 返回字符串中 `(string.match(/^### /gm) ?? []).length === 27`（27 个 tool 各一个 `### tool_name`）
  3. 返回字符串包含且只包含这 7 个 `## ` group 头：`## Ping`, `## Lifecycle`, `## Observe`, `## Interaction`, `## Compound`, `## System`, `## VLM`（用 `string.match(/^## /gm)?.length === 7` + 七个 substring 全命中）
- 先跑测试，**必须看到红**：
  ```bash
  cd /Users/doracawl/workspace/goliajp/simx/web && bun x vitest run src/__tests__/tools-generator.test.ts 2>&1 | tail -10
  ```
  **期望**：exit ≠ 0，输出含 `FAIL`，原因：`generate-tools-page.ts` 不存在

**绿（实现）**
- 文件：`web/scripts/generate-tools-page.ts`
- API：
  ```ts
  export default async function generateToolsPage(): Promise<string>
  // 当作为 CLI 直接 `bun web/scripts/generate-tools-page.ts` 跑时，
  // 在文件末尾 `import.meta.main` 守卫下：调用 generateToolsPage()，
  // 写入 path.resolve(import.meta.dirname, '../content/tools.mdx')，exit 0
  ```
- 关键点：
  - 用 `import('../../src/mcp/tools.ts')` 读 `ALL_TOOLS`（bun 已验证跨目录直接跑 .ts，见探测）
  - 硬编码 group 映射：name → group（与 README §Status table 同口径：`ping` → Ping；`simulator_list/boot/shutdown/app_launch/terminate/install/uninstall` → Lifecycle；`screen_describe/screenshot/hierarchy/element_inspect` → Observe；`tap/double_tap/long_press/fill/swipe/scroll_to/key_press` → Interaction；`find_and_tap/wait_for/flow_run` → Compound；`open_url/pasteboard_set/pasteboard_get/permissions_grant` → System；`explain_screen` → VLM）
  - 每个 tool 渲染：`### {name}` + 一段 `{description}` + ` ```json` fenced block 写 `JSON.stringify(inputJsonSchema, null, 2)` + 空行
  - mapping 中**任何**未命名的 tool → 抛 `Error(\`unmapped tool: ${name}\`)`（防止 simx 主侧新增 tool 后 web 端 silent skip）
- 跑测试：
  ```bash
  cd /Users/doracawl/workspace/goliajp/simx/web && bun x vitest run src/__tests__/tools-generator.test.ts 2>&1 | tail -10
  ```
  **期望**：exit 0，输出含 `3 passed`

**重构**：跳过（C2 工作量已大，generator 内部抽象推到 C3 必要时）

### S2. 3 个内容 mdx + nav 扩到 5 + 跑一次 generator 写出 tools.mdx

**动作 + 检测**
- 文件 1：`web/content/plugin-install.mdx`
  - frontmatter `title: Plugin install`
  - body：把 `docs/plugin-install.md` 全文（66 行）原样粘贴；末尾追加一段 `> Source of truth: [docs/plugin-install.md on GitHub](https://github.com/goliajp/simx/blob/develop/docs/plugin-install.md)`
- 文件 2：`web/content/authoring.mdx`
  - frontmatter `title: Authoring guide for AI agents`
  - body：把 `README.md` line 134–245（`## Authoring guide for AI agents` 整段，到下一 `---` 分隔线前）原样粘贴；mdx 内 `## Authoring guide for AI agents` 顶头改成 `# Authoring guide for AI agents`（h1）以避免与 sidebar title 重复。末尾追加 `> Source of truth: [README §Authoring on GitHub](https://github.com/goliajp/simx#authoring-guide-for-ai-agents)`
- 文件 3：`web/content/quick-start.mdx`
  - 替换 C1 占位
  - frontmatter `title: Quick start`
  - body：3 段
    1. h1 + 一段 "why another one" 浓缩（README line 7–25 浓缩到 4-6 行散文）
    2. h2 `Install` —— README §Quick start `claude --plugin-dir` block + `Local dev (clone + bun)` block 原样粘贴
    3. h2 `Example` —— README line 110–124 example block 原样粘贴
  - 末尾追加 `> Full README: [github.com/goliajp/simx](https://github.com/goliajp/simx)`
- 文件 4：`web/content/nav.config.ts` 扩到 5 项：
  ```ts
  const NAV: NavItem[] = [
    { slug: 'quick-start',    title: 'Quick start' },
    { slug: 'plugin-install', title: 'Plugin install' },
    { slug: 'authoring',      title: 'Authoring guide' },
    { slug: 'tools',          title: 'MCP tools (27)' },
    { slug: 'examples',       title: 'Examples' },  // C3 才有 mdx; 现在点击落到 docs-local 404
  ]
  export default NAV
  ```
- 文件 5：第一次跑 generator 实际写出 `web/content/tools.mdx`：
  ```bash
  cd /Users/doracawl/workspace/goliajp/simx/web && bun scripts/generate-tools-page.ts
  ```

**检测命令**：
```bash
cd /Users/doracawl/workspace/goliajp/simx/web && \
  test -f content/plugin-install.mdx && \
  test -f content/authoring.mdx && \
  test -f content/tools.mdx && \
  grep -c '^### ' content/tools.mdx && \
  grep -c '^- ' /dev/null; \
  grep -c "slug: '" content/nav.config.ts
```
**期望**：
- 前 3 个 `test -f` 全 pass
- `grep -c '^### ' content/tools.mdx` 输出 `27`
- `grep -c "slug: '" content/nav.config.ts` 输出 `5`
- 整体 exit 0

### S3. 接 prebuild + home CTA + 全绿 gate

**红（写测试）**
- 文件：`web/src/views/__tests__/home-docs-cta.test.tsx`（新）
- 断言（**1 个 case**）：渲染 `<HomeView/>`（用 `MemoryRouter` 包），DOM `textContent` 含 `Read the docs`；且 `screen.getByText(/Read the docs/i).closest('a')!.getAttribute('href')` 等于 `/docs/quick-start`（或包含 `/docs`，宽松）
- 先跑测试，**必须看到红**：
  ```bash
  cd /Users/doracawl/workspace/goliajp/simx/web && bun x vitest run src/views/__tests__/home-docs-cta.test.tsx 2>&1 | tail -8
  ```
  **期望**：exit ≠ 0

**绿（实现）**
- 文件 1：`web/src/views/home.tsx`
  - 三个 `LINKS[].href` 改为 internal route：`/docs/quick-start` / `/docs/plugin-install` / `/docs/authoring`；把 `<a href target="_blank">` 改成 `<Link to>`（`react-router`）；删 `rel/target`
  - hero `<p>` 下方加一个 `<Link to="/docs/quick-start">` 突出 CTA（GlassCard 或 button-shape），文本 `Read the docs →`
- 文件 2：`web/package.json`
  - `scripts.prebuild`：`"bun scripts/generate-tools-page.ts"`
  - `scripts.build`：`"bun run prebuild && tsc --noEmit && prettier --write src/ && vite build"`
- 跑：
  ```bash
  cd /Users/doracawl/workspace/goliajp/simx/web && bun x vitest run src/views/__tests__/home-docs-cta.test.tsx 2>&1 | tail -8
  ```
  **期望**：exit 0，`1 passed`

**重构**：跳过

## Checkpoint C2 验收

```bash
cd /Users/doracawl/workspace/goliajp/simx/web && \
  rm -f content/tools.mdx && \
  bun run prebuild 2>&1 | tee /tmp/c2-prebuild.log | tail -3 && \
  test -f content/tools.mdx && \
  test "$(grep -c '^### ' content/tools.mdx)" = "27" && \
  test -f content/plugin-install.mdx && \
  test -f content/authoring.mdx && \
  test "$(grep -c "slug: '" content/nav.config.ts)" = "5" && \
  bun x vitest run 2>&1 | tee /tmp/c2-test.log | tail -3 && \
  grep -qE 'Tests +[0-9]+ passed' /tmp/c2-test.log && \
  ! grep -qE 'Tests +[0-9]+ failed' /tmp/c2-test.log && \
  bun run build 2>&1 | tee /tmp/c2-build.log | tail -5 && \
  grep -q 'built in' /tmp/c2-build.log && \
  echo C2_PASS
```
**期望**：最末输出含 `C2_PASS`，exit 0。
- `rm + prebuild` 验证 generator 从零再生 → tools.mdx 重现且仍是 27 个 `### `
- 4 个 mdx 文件都在
- nav 5 项
- 全部 vitest 套件通过且没有 failed 行
- vite build 完成

任一上游 exit ≠ 0 或 grep 未命中 = C2 未通过。

## 完成后动作

1. `mv docs/plan-hot.md docs/plan-history/web-v0.2-c2-hot.md`
2. 调 sub-agent 按 CLAUDE.md §6 模板生成新 `plan-hot.md`（web v0.2 C3：`/docs/examples` index + 3 example sub-page + shiki runtime 激活）
3. **不**自行展开 C3；等用户开口
