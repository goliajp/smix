# plan-cold/web-v0.1.md — simx 官网 v0.1：拿 simx.golia.jp + 占位单页上线

> [4/4 信息层] 一个独立 sub-project 的 minor 版本。**`web/` 子项目不进 simx 主版本序列**（v0.x / v1.x / v2 是 simx core；`web/` 用独立 web-v0.x 节奏）。

## 为什么要做

simx v1.0 已 release on GitHub。除 README + docs/ markdown 之外还需要一个 web 入口：
- 给 Claude Code 用户搜索 "simx" 时第一个可看的 landing
- 给将来"开源给人安全感"的可信门面（用户原话）
- 后续 v0.2 承载完整 docs site（Authoring guide / 27-tool reference / Quick start / Examples），目前 GitHub README 承载得勉强

v0.1 不放 docs 内容，只立**域名 + 占位单页**——让 simx.golia.jp 这条 URL 先可达 + HTTPS。

## 入口条件

```bash
# v1.0 release ready
test -f /Users/doracawl/workspace/goliajp/simx/.devops || true  # 允许不存在，本期建
gh repo view goliajp/simx --json visibility -q '.visibility' | grep -q PUBLIC

# devops 通路在场
test -f ~/.config/devops/api-key
curl -sf -H "Authorization: Bearer $(cat ~/.config/devops/api-key)" https://devops.golia.jp/api/dns/zones > /dev/null

# starter 脚手架可读
test -d ~/workspace/goliajp/devops/starters/web/src
```

## 资源依赖

- 环境：bun（已用于 simx 主项目）/ Node 22+（vite 8 + tailwind 4）
- 外部：Cloudflare DnsStore（PUT /api/dns/zones/golia.jp）+ devops-server caddy action
- Target device：t01（aws-tokyo，Debian arm64，Caddy + ACME）
- 设计系统：`@goliapkg/gds` v2.2（GOLIA 自家，已被多个 golia.jp 子域使用）

## 已知风险

- **starter 含 demo views + GitHub API 调用** → cp 后必须删 `src/views/{about,components,state,home.test}.tsx` + `src/api/`，否则空 GitHub API call 把 home 渲染搞坏
- **starter base path** = `/starters/web/` → v0.1 必须改 `vite.config.ts base: '/'` + `main.tsx` 删 basename，否则 build 出来的 dist 资源路径找不到
- **DNS 传播延迟** → Cloudflare typically < 30s 但首次可能要 1-2min；Caddy ACME 拿证可能要 30-60s
- **devops CLI `dns add` 命令缺** → 已通过 HTTP API 直 PUT zone 解决（read-modify-write 整个 zone records[]）；未来如果加新子域多了，可以补 `devops dns endpoint set` CLI

## TDD 要点

非传统 TDD（v0.1 没业务逻辑测试）。验证依赖部署后 e2e 检测：

```bash
# DNS resolution
dig +short A simx.golia.jp @1.1.1.1 | tail -1  # → 18.179.107.143 (t01)

# HTTPS reachable
curl -sI https://simx.golia.jp | head -1       # → HTTP/2 200

# SPA 服务正确
curl -s https://simx.golia.jp/anything | grep -q 'id="root"'  # SPA fallback 命中
```

后续 v0.2 docs 内容会有 TypeScript / 路由 / MDX 渲染测试。

## Checkpoint 概要列表

- **C1**：占位单页上线 — DNS record + Caddy site + v1.0 简介 + 3 个 CTA card + 27-tool 分组 badge。`bash` 验证三条 dig/curl 全通。
- **C2**（v0.2 主体）：docs framework — sidebar + topbar + MDX 加载 + 4 子页（Quick start / Plugin install / Authoring / Tool reference）。后续单独 cold。
- **C3**（v0.3）：demo 段 — asciinema 或 screencast，占位 hero 上方 fold。

## 出口验收（v0.1）

C1 验证三条命令全通 + 浏览器手感看一次（home + 404 fallback + theme toggle）。

## 触发热化的 prompt 模板

按 CLAUDE.md §6 模板。本期特殊 context：

- `web/` 子目录用 bun（与 simx 主项目同），但**独立 lockfile**（`web/bun.lock`）—— 不混入 simx 主项目 deps
- vite 8 + tailwind 4 + React 19 + GDS 2.2 是 starter 默认 stack，**不**降级
- v0.2 起做 docs framework，先评估"starter + MDX 自实现 sidebar/route" vs "切 vitepress"——核心是 docs，工程量大，**值得另起一个 plan-cold/web-v0.2.md 单独评估**
