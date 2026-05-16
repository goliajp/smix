# plan-hot — web v0.2 到 C3：examples 索引 + 3 sub-page + shiki build-time 高亮

> [3/4 信息层] **target = `web/` sub-project v0.2 C3**（不是 simx 主项目）。范围严格止于 examples 索引 mdx + 3 example sub-page + `@shikijs/rehype` build-time 接入。**不**做 docs ↔ simx-repo 自动 sync（推 v0.3+）、**不**做 search、**不**切 vitepress。完成后归档到 `docs/plan-history/web-v0.2-c3-hot.md`。

## 目标 checkpoint

**web v0.2 C3**：examples 段上线 + 全 mdx fenced code block 经 shiki 静态高亮。世界变成：

1. `web/` 新增 1 dev dep：`@shikijs/rehype`（已用的 `shiki@^4.0.2` 同 major；探测确认 v4.0.2 存在）
2. `vite.config.ts` mdx plugin 的 `rehypePlugins` 追加 `@shikijs/rehype`，配置 `{ themes: { light: 'github-light', dark: 'github-dark' } }`（dual-theme，配合现有 `.dark` class）。**`rehypeShiki` 必须排在 `rehypeSlug` 之后、`rehypeAutolinkHeadings` 之前**（slug 先稳定锚点，shiki 改 code AST 不动 heading id，autolink 最后挂 anchor）
3. `web/content/examples.mdx`（新）：3 张卡片导引到 sub-page；纯 mdx + markdown，无 React 组件依赖（卡片用 `<a>` + Tailwind utility，避免引入新组件）
4. `web/content/examples-login-tap.mdx` / `examples-tap-text-selector.mdx` / `examples-screenshot-only.mdx`（新）：每页 = h1 标题 + 1 段简介 + ` ```ts ` fenced 完整源码（手 copy 自 `examples/*.test.ts`）+ 2-4 段"解读" + "more in repo" link
5. `web/content/nav.config.ts` 已含 `examples` 项（C2 落），**不**新增 sub-page sidebar 条目（决策：避免 sidebar 长尾，examples 索引页承担二级导航）
6. vitest 通过：4 个新 mdx 都能 dynamic import 渲染；examples 索引页 DOM 含 3 个 sub-page link；至少 1 个 example sub-page 渲染结果含 shiki 注入的 `class="shiki"` 节点

## 前置条件

```bash
# 仍在 web v0.2 feature 分支
test "$(git rev-parse --abbrev-ref HEAD)" = "feature/web-v0.2"

# C1/C2 已 close
test -f docs/plan-history/web-v0.2-c1-hot.md
test -f docs/plan-history/web-v0.2-c2-hot.md

# C2 产出齐全
test -f web/content/quick-start.mdx
test -f web/content/plugin-install.mdx
test -f web/content/authoring.mdx
test -f web/content/tools.mdx
test "$(grep -c "slug: '" web/content/nav.config.ts)" = "5"

# 3 example 源文件在
test -f examples/login-tap.test.ts
test -f examples/tap-text-selector.test.ts
test -f examples/screenshot-only.test.ts

# C3 目标文件还未存在
test ! -e web/content/examples.mdx
test ! -e web/content/examples-login-tap.mdx
test ! -e web/content/examples-tap-text-selector.mdx
test ! -e web/content/examples-screenshot-only.mdx
grep -q '"@shikijs/rehype"' web/package.json && exit 1 || true
```

## 步骤（线性，无分叉；3 步）

### S1. 装 `@shikijs/rehype` + 接通 mdx 编译期高亮（TDD 严格红→绿）

**红（写测试）**
- 文件：`web/src/views/docs/__tests__/shiki-rehype.test.tsx`（新）
- 断言（**1 个 case**）：先 `await import('../../../../content/quick-start.mdx')`，`render(<Mdx/>)` 后断言容器内至少一个 `pre` 元素带 `class` 含 `shiki`（`container.querySelector('pre.shiki')` 非 null）；并断言至少一个 `span` 带 inline `style` 含 `color`（shiki 注入的 token span 样式）
- 先跑测试，**必须看到红**：
  ```bash
  cd /Users/doracawl/workspace/goliajp/simx/web && bun x vitest run src/views/docs/__tests__/shiki-rehype.test.tsx 2>&1 | tail -10
  ```
  **期望**：exit ≠ 0，输出含 `FAIL`，原因：mdx 编出的 fenced block 是 plain `<pre><code>`，无 `class="shiki"`

**绿（实现）**
- 命令（装 dep；同 shiki 同 major，避免 peer drift）：
  ```bash
  cd /Users/doracawl/workspace/goliajp/simx/web && bun add --dev @shikijs/rehype
  ```
- 文件：`web/vite.config.ts`
  - top-import：`import rehypeShiki from '@shikijs/rehype'`
  - mdx 配置 `rehypePlugins` 改为：
    ```ts
    [rehypeSlug, [rehypeShiki, { themes: { light: 'github-light', dark: 'github-dark' } }], [rehypeAutolinkHeadings, { behavior: 'append' }]]
    ```
- 跑测试：
  ```bash
  cd /Users/doracawl/workspace/goliajp/simx/web && bun x vitest run src/views/docs/__tests__/shiki-rehype.test.tsx 2>&1 | tail -10
  ```
  **期望**：exit 0，输出含 `1 passed`

**重构**：跳过（rehype 链 order 即文档化注释，本身就一行；不抽 wrapper）

### S2. examples 索引 mdx + 3 sub-page mdx（TDD 严格红→绿）

**红（写测试）**
- 文件：`web/src/views/docs/__tests__/examples-pages.test.tsx`（新）
- 断言（**3 个 case**）：
  1. `await import('../../../../content/examples.mdx')` → `render(<Mdx/>)` 后 DOM 含**全部 3 个**`href` 串：`/docs/examples-login-tap` / `/docs/examples-tap-text-selector` / `/docs/examples-screenshot-only`（用 `container.querySelectorAll('a[href]')` 收集 href，`expect(hrefs).toEqual(expect.arrayContaining([...]))`）
  2. `await import('../../../../content/examples-login-tap.mdx')` → render 后 DOM `textContent` 含字面 `app.tap({ text: 'General' })`（验证完整源码已嵌入）
  3. `await import('../../../../content/examples-screenshot-only.mdx')` → render 后 DOM `textContent` 含字面 `app.screenshot()` 且含字面 `com.apple.mobilesafari`
- 先跑测试，**必须看到红**：
  ```bash
  cd /Users/doracawl/workspace/goliajp/simx/web && bun x vitest run src/views/docs/__tests__/examples-pages.test.tsx 2>&1 | tail -10
  ```
  **期望**：exit ≠ 0，输出含 `FAIL`，原因：4 个 mdx 都未创建

**绿（实现）**
- 文件 1：`web/content/examples.mdx`
  - frontmatter `title: Examples`
  - body：h1 `# Examples` + 1 段简介（"Real, runnable AI-authored test samples..."，仿 `examples/README.md`）
  - 3 张卡片（纯 markdown link list，每项格式 `- [{title}](/docs/examples-{slug}) — {what it exercises 一句话}`，链接路径**字面**匹配 sub-page slug）
  - 末尾 `> Source: [examples/ on GitHub](https://github.com/goliajp/simx/tree/develop/examples)`
- 文件 2：`web/content/examples-login-tap.mdx`
  - frontmatter `title: 'login-tap: selector showcase'`
  - body：h1 + 1 段简介（"v0.3 selector resolver showcase: text / id / role+name / `inside` modifier"）
  - ` ```ts ` fenced：把 `examples/login-tap.test.ts` 全文（30 行）原样粘贴
  - 解读 2-4 段（每段对应源码内 `// 1) ... // 5)` 注释的展开：base text / RegExp / waitFor / id selector / `inside` modifier）
  - 末尾 `> Full file: [examples/login-tap.test.ts on GitHub](https://github.com/goliajp/simx/blob/develop/examples/login-tap.test.ts)`
- 文件 3：`web/content/examples-tap-text-selector.mdx`
  - frontmatter `title: 'tap-text-selector: minimal smoke'`
  - body：h1 + 简介（"v0.2 closing-checkpoint smoke. Single `app.tap({text:'General'})` against pre-launched Settings."）
  - ` ```ts ` fenced：`examples/tap-text-selector.test.ts` 全文（11 行）原样粘贴
  - 解读 2 段（为何不 explicit launch / 为何依赖 runner auto-launch）
  - 末尾 GitHub link
- 文件 4：`web/content/examples-screenshot-only.mdx`
  - frontmatter `title: 'screenshot-only: v0.1 baseline'`
  - body：h1 + 简介（"v0.1 milestone. No HID / runner needed — just sim boot + `app.launch` + `app.screenshot()`."）
  - ` ```ts ` fenced：`examples/screenshot-only.test.ts` 全文（12 行）原样粘贴
  - 解读 2 段（PNG magic bytes 验证 / 为何 v0.1 不需要 runner）
  - 末尾 GitHub link
- 跑测试：
  ```bash
  cd /Users/doracawl/workspace/goliajp/simx/web && bun x vitest run src/views/docs/__tests__/examples-pages.test.tsx 2>&1 | tail -10
  ```
  **期望**：exit 0，输出含 `3 passed`

**重构**：跳过（4 个 mdx 是内容，不抽组件；卡片样式留到未来若加更多 examples 再说）

### S3. 全套件 + typecheck + build 全绿 gate

**动作**
- 不再新增/改任何 src 文件；本步只跑 gate
- 检查 build 产物：mdx pages 各自仍是独立 chunk（lazy import），shiki 静态高亮已 inline 进 mdx chunk（不应单独产生 shiki async chunk；若产生 > 1MB 单 chunk 即不合预期）

**检测命令**：
```bash
cd /Users/doracawl/workspace/goliajp/simx/web && \
  bun x vitest run 2>&1 | tail -5 && \
  bun run check 2>&1 | tail -5 && \
  bun run build 2>&1 | tail -10 && \
  ls -la dist/assets/ | awk '{print $5, $9}' | sort -rn | head -10
```
**期望**：
- vitest：所有套件 pass，无 `failed` 行
- `bun run check`：exit 0，无 `error TS`
- `bun run build`：exit 0，输出含 `built in`
- `dist/assets/` 单文件 ≤ 1.5MB（main bundle 当前 ~498k，加 shiki inline grammar 估计 +200-500k 不超 1.5M）

## Checkpoint C3 验收

```bash
cd /Users/doracawl/workspace/goliajp/simx/web && \
  test -f content/examples.mdx && \
  test -f content/examples-login-tap.mdx && \
  test -f content/examples-tap-text-selector.mdx && \
  test -f content/examples-screenshot-only.mdx && \
  grep -q '"@shikijs/rehype"' package.json && \
  bun x vitest run 2>&1 | tee /tmp/c3-test.log | tail -3 && \
  grep -qE 'Tests +[0-9]+ passed' /tmp/c3-test.log && \
  ! grep -qE 'Tests +[0-9]+ failed' /tmp/c3-test.log && \
  bun run check 2>&1 | tee /tmp/c3-check.log | tail -3 && \
  ! grep -qE 'error TS' /tmp/c3-check.log && \
  bun run build 2>&1 | tee /tmp/c3-build.log | tail -5 && \
  grep -q 'built in' /tmp/c3-build.log && \
  test "$(find dist/assets -type f -size +1500k | wc -l | tr -d ' ')" = "0" && \
  echo C3_PASS
```
**期望**：最末输出含 `C3_PASS`，exit 0。任一上游 exit ≠ 0 / grep 未命中 / 出现 >1.5MB 单 chunk = C3 未通过。

## 完成后动作

1. `mv docs/plan-hot.md docs/plan-history/web-v0.2-c3-hot.md`
2. C3 是 web v0.2 的最后 checkpoint —— 跑 cold plan §出口验收 全量 sweep（5 个 `/docs/*` 路径 + 27 tool 行数）；通过则关闭 web v0.2，等用户开口启动 web v0.3 冷计划
3. **不**自行展开 web v0.3；等用户开口
