# plan-cold/web-v0.2.md — simx.golia.jp v0.2：docs framework + 4 主页内容

> [4/4 信息层] web sub-project 第二个 minor。v0.1 占位单页已上线，v0.2 让它真正承载 docs。

## 为什么要做

`v1.md` §4 SC[1] 字面 "Claude Code 拿 README 'Authoring guide' + MCP server，0-shot 写出跑通的测试" —— Authoring guide 是 simx 核心资产，**目前只在 README** 里（112 行）。README 形态做不了：sidebar 导航 / TOC / 锚点 deep-link / dark mode 代码高亮 / mobile sticky nav。Docs site 解决这些 + 给 simx.golia.jp 真正的内容（v0.1 只是 placeholder）。

## 入口条件

```bash
# v0.1 已 close
test -f docs/plan-history/web-v0.1-c1-hot.md
curl -sf -o /dev/null https://simx.golia.jp                 # 上线在场

# 既有 docs 资产
grep -q '^## Authoring guide for AI agents' README.md       # 主要内容源
test -f docs/plugin-install.md && wc -l docs/plugin-install.md   # 66 行
ls examples/*.test.ts | wc -l                                # ≥ 3
test -f src/mcp/tools.ts                                     # 27 tool spec
```

## 资源依赖

- `@mdx-js/rollup` v3 + `remark-gfm`（github-flavored）+ `rehype-slug` + `rehype-autolink-headings`（TOC 锚点）
- `shiki` v1 做代码高亮（VS Code 同源 tokenizer，质量好；async 加载小心）
- `gray-matter`（frontmatter 解析）
- 沿用 GDS theme + Tailwind 4 + react-router 7（已在 starter）

不引入：docs framework（vitepress/docusaurus —— 用户明示用 starter）/ Algolia search / 后端

## 已知风险

- **MDX SoT vs simx/docs/*.md 双源漂走** —— **决策**：web docs 内容文件落 `web/content/*.mdx`，simx 主 repo `README.md` / `docs/plugin-install.md` 是 source-of-truth；v0.2 期 manual sync 一次性把 README §Authoring 段 + plugin-install.md copy-paste 到 mdx。**未来同步策略**（脚本拉/build 时 transform）推到 v0.3+
- **27-tool reference 同步** —— 27 tool 名 + schema 真 SoT 是 `src/mcp/tools.ts`；docs page 用 build-time 脚本生成 mdx（vite plugin/prebuild script），**不**手抄
- **MDX 在 Vite 8 上的稳定性** —— @mdx-js/rollup v3 + vite 8 配合需验，可能要额外 vite plugin wrap
- **sidebar nav config** —— **决策**：手写 `web/content/nav.config.ts`（4 主页+ 1 examples 索引，扁平），不走 fs-walk
- **mobile responsive** —— sidebar 折叠为 drawer，topbar 高 12，main 区 max-w 3xl

## TDD 要点

`web/` 子项目用 vitest（starter 自带）：

- C1 unit：assert mdx 文件能 import 出 React component（一个 sample mdx + dynamic import 测试）
- C2 unit：assert nav.config 的 4 个 path 都能找到对应 mdx
- C3 build-time：27-tool reference 生成器输出条数 = `import("../../src/mcp/tools.ts").DEFAULT_TOOLS.length`

E2E 暂不引入 playwright（v0.3 demo 时再说）；C1-C3 关闭用 `bun run build` + 浏览器手感 + `curl https://simx.golia.jp` 抓 HTML grep 关键 token。

## Checkpoint 概要列表

- **C1**：docs framework 骨架 —— MDX loader 配通 + sidebar/topbar route + dark mode + 1 page（`/docs/quick-start`）+ nav.config + 404 fallback；老 home 不动
- **C2**：4 主页内容 —— `/docs/quick-start` / `/docs/plugin-install` / `/docs/authoring` / `/docs/tools`（27-tool reference 走 build-time generator from `src/mcp/tools.ts`）；home 加 "Read the docs" 突出 CTA
- **C3**：examples 段 —— `/docs/examples`（3 examples 嵌 shiki 高亮代码 + 解读），每个 example 单独 sub-page

## 出口验收

```bash
# build + test 全过
cd web && bun run build && bun run test

# 真 site 验证（部署后）
for path in /docs/quick-start /docs/plugin-install /docs/authoring /docs/tools /docs/examples; do
  code=$(curl -sk -o /dev/null -w "%{http_code}" "https://simx.golia.jp${path}")
  echo "${path} ${code}"
done | grep -v ' 200'   # 期望全 200，grep -v 输出应为空

# 27-tool reference 生成正确条数
curl -s https://simx.golia.jp/docs/tools | grep -oE '<h3[^>]*>([a-z_]+)</h3>' | wc -l  # 期望 27
```

## 触发热化的 prompt 模板

按 CLAUDE.md §6 标准模板。本期特殊 context：

- C1 是骨架，**MDX vite plugin 配通是 C1 主要风险**——sub-agent 热化时先做 `bun add @mdx-js/rollup remark-gfm shiki gray-matter rehype-slug rehype-autolink-headings` 探 vite 8 兼容性，发现破裂立即回报，不要自行降级 vite
- **C2 27-tool generator** 是 build-time prebuild script（不是 runtime），形态先决（写在 `web/scripts/generate-tools-page.ts`，npm script `prebuild` 跑），不要做 lazy/runtime fetch
- 主对话已经决策的事**不要再问**：docs 内容文件位置 `web/content/*.mdx` / sidebar 手写 / shiki 而非 prism / 无 search
