# plan-hot — web v0.2 到 C1：MDX 骨架 + 占位 quick-start page

> [3/4 信息层] **target = `web/` sub-project v0.2 C1**（不是 simx 主项目 v1.1）。覆盖范围严格止于 docs framework 骨架；**不**写真实文档内容（C2）、**不**做 27-tool generator（C2）、**不**写 examples（C3）。完成后归档到 `docs/plan-history/web-v0.2-c1-hot.md`。

## 目标 checkpoint

**web v0.2 C1**：MDX docs framework 骨架接通。世界变成：

1. `web/` 新增 6 dev deps：`@mdx-js/rollup` `remark-gfm` `shiki` `gray-matter` `rehype-slug` `rehype-autolink-headings`
2. `vite.config.ts` 接入 `@mdx-js/rollup` plugin（排在 `react()` 前），`.mdx` 可直接 import 出 React component
3. 路由表新增 `/docs/:slug` + sidebar layout（topbar 沿用、theme toggle 沿用、dark mode 沿用），slug 不命中走 docs-local 404 view（**老 home `/` 不动**）
4. `web/content/quick-start.mdx` 占位（frontmatter `title` + 一段 `## Hello docs` + 一个 fenced code block 验 shiki/gfm 路径已激活；**真内容在 C2**）
5. `web/content/nav.config.ts` 手写导出 `NavItem[]` 含且只含 `quick-start` 一项
6. vitest 通过：mdx 文件能 dynamic import 渲染出 `Hello docs` + nav.config 第一项 slug 解析到 mdx + docs-local 404 命中

## 前置条件

```bash
# 在 web v0.2 feature 分支上
test "$(git rev-parse --abbrev-ref HEAD)" = "feature/web-v0.2"

# v0.1 已 close
test -f docs/plan-history/web-v0.1-c1-hot.md
curl -sf -o /dev/null https://simx.golia.jp

# starter 仍是 v0.1 状态
test -f web/src/views/home.tsx
test ! -e web/content                                   # 不能已存在
grep -q '"@mdx-js/rollup"' web/package.json && exit 1 || true
```

## 步骤（线性，无分叉；3 步）

### S1. 装依赖 + 接通 MDX vite plugin

**动作 + 检测**
- 文件：`web/package.json`（bun 改动）+ `web/vite.config.ts`
- 命令：
  ```bash
  cd web && bun add --dev @mdx-js/rollup remark-gfm shiki gray-matter rehype-slug rehype-autolink-headings
  ```
- `vite.config.ts` 改动：top-import `import mdx from '@mdx-js/rollup'`、`import remarkGfm from 'remark-gfm'`、`import rehypeSlug from 'rehype-slug'`、`import rehypeAutolinkHeadings from 'rehype-autolink-headings'`；`plugins: [tailwindcss(), mdx({ remarkPlugins: [remarkGfm], rehypePlugins: [rehypeSlug, [rehypeAutolinkHeadings, { behavior: 'append' }]] }), react()]`。**mdx plugin 必须排在 `react()` 前**（plugin-react v6 负责对 mdx 编译出的 JSX 做 fast-refresh）
- 占位文件：`web/content/_probe.mdx` 写一行 `# probe`（仅用于检测 import 路径能成立；S3 删除）
- TS 端：`web/src/mdx.d.ts` 加 `declare module '*.mdx' { const C: import('react').ComponentType; export default C; export const frontmatter: Record<string, unknown> | undefined }`

**检测命令**：
```bash
cd /Users/doracawl/workspace/goliajp/simx/web && bun run build 2>&1 | tail -5
```
**期望**：exit 0，输出含 `built in`。不期望任何 `Failed to resolve` / `Cannot find module` / mdx parser 报错。

### S2. 写 docs 路由 + sidebar layout + 404，先红再绿

**红（写测试）**
- 文件：`web/src/views/docs/__tests__/docs-route.test.tsx`（新）
- 断言（**3 个 case**）：
  1. `import('@/../content/quick-start.mdx')` 解出的 default export 在 jsdom 里能 `render()`（@testing-library/react），输出含 `Hello docs`
  2. `import('@/../content/nav.config')` 的 default 长度 ≥ 1 且第一项 `slug === 'quick-start'`
  3. 用 `createMemoryRouter([...routes], { initialEntries: ['/docs/__nope__'] })` + `<RouterProvider router={mr}/>` 渲染，断言 DOM `textContent` 含 `Not found`（docs-local 404，**不**走 SPA root 重定向）
- 先跑测试，**必须看到红**：
  ```bash
  cd /Users/doracawl/workspace/goliajp/simx/web && bun x vitest run src/views/docs/__tests__/docs-route.test.tsx 2>&1 | tail -10
  ```
  **期望**：exit ≠ 0，输出含 `FAIL`，原因：`quick-start.mdx` / `nav.config` / docs route 均未创建

**绿（实现）**
- 文件 1：`web/content/nav.config.ts`
  ```ts
  export type NavItem = { slug: string; title: string }
  const NAV: NavItem[] = [{ slug: 'quick-start', title: 'Quick start' }]
  export default NAV
  ```
- 文件 2：`web/content/quick-start.mdx`（替换 S1 的 `_probe.mdx`；删 probe）
  - frontmatter：`title: Quick start`
  - body：`## Hello docs` + 一段 placeholder 段落 + 一个 ` ```ts ` fenced code block（验 shiki/gfm 路径激活；C1 不主动调用 shiki 高亮 runtime）
- 文件 3：`web/src/views/docs/layout.tsx`（sidebar from `content/nav.config.ts` + `<Outlet/>`；**外层仍是 `AppLayout`**，本 layout 只在 main 区内部分 left-sidebar / right-content 两栏）
- 文件 4：`web/src/views/docs/page.tsx`（`useParams<{ slug: string }>()` + `import.meta.glob('../../../content/*.mdx', { eager: false })` 取对应 mdx；slug 找不到 → 渲染 `<DocsNotFound />`，**不** `<Navigate to="/" />`）
- 文件 5：`web/src/views/docs/not-found.tsx`（一段 `Not found` 文案 + 回 `/docs/quick-start` 链接）
- 文件 6：`web/src/main.tsx` 路由表追加 `/docs` 段（`index → <Navigate replace to="/docs/quick-start"/>` + `:slug → <DocsPage/>`），**`*` catchall 仍指向 `Navigate to="/"`，不动**
- 跑测试：
  ```bash
  cd /Users/doracawl/workspace/goliajp/simx/web && bun x vitest run src/views/docs/__tests__/docs-route.test.tsx 2>&1 | tail -10
  ```
  **期望**：exit 0，输出含 `3 passed`

**重构**：跳过（C1 是骨架，避免过早抽象 mdx-loader hook）

### S3. typecheck + build 全绿，删 probe

**动作**
- 删 `web/content/_probe.mdx`（S1 占位，S2 已被 `quick-start.mdx` 取代）
- 不动其他文件

**检测命令**：
```bash
cd /Users/doracawl/workspace/goliajp/simx/web && bun run check 2>&1 | tail -5 && bun run build 2>&1 | tail -5
```
**期望**：两段都 exit 0；`bun run check` 无 `error TS`；`bun run build` 输出含 `built in`。

## Checkpoint C1 验收

```bash
cd /Users/doracawl/workspace/goliajp/simx/web && \
  bun x vitest run src/views/docs/__tests__/docs-route.test.tsx 2>&1 | tee /tmp/c1-test.log | tail -3 && \
  bun run build 2>&1 | tee /tmp/c1-build.log | tail -3 && \
  grep -q '3 passed' /tmp/c1-test.log && \
  grep -q 'built in' /tmp/c1-build.log && \
  test -f content/quick-start.mdx && \
  test -f content/nav.config.ts && \
  test ! -e content/_probe.mdx && \
  echo C1_PASS
```
**期望**：最末输出含 `C1_PASS`，exit 0。任一上游命令 exit ≠ 0 或 grep 未命中 = C1 未通过。

## 完成后动作

1. `mv docs/plan-hot.md docs/plan-history/web-v0.2-c1-hot.md`
2. 调 sub-agent 按 CLAUDE.md §6 模板生成新 `plan-hot.md`（web v0.2 C2：4 主页内容 + 27-tool build-time generator）
3. **不**自行展开 C2；等用户开口
