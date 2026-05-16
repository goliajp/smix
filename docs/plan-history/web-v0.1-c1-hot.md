# plan-hot — web v0.1 C1：拿 simx.golia.jp + 占位单页上线

> [3/4 信息层] 当前唯一热计划。**target = `web/` sub-project v0.1 C1**（不是 simx 主版本 v1.1）。完成后归档到 `docs/plan-history/web-v0.1-c1-hot.md`，新 plan-hot 由 main convo 决定写 web v0.2（docs framework）还是 simx 主项目 v1.1（Watch mode / Cell L4）—— **优先级未定，等用户开口**。
>
> **C1 特殊性**：此 plan-hot 在 S1-S5 已落地后才补写（参见 CLAUDE.md §9 不变量 #7 "4 层始终保持"——为修复 v0.7 C7 close 后 plan-hot 副本残留 + web sub-project 起手两件事一起补）。S6-S7 仍未做，是 right-now-doing 段。

## 目标 checkpoint

**web v0.1 C1**：simx.golia.jp 上线占位单页 + HTTPS。世界变成：

1. DNS：simx.golia.jp CNAME → t01.golia.jp.（已 ✓ — Cloudflare 2026-05-16 PUT 完）
2. 子目录 `web/` 在 simx repo 内，bun 独立 lockfile
3. Caddy 在 t01 服务 `/var/lib/simx-web/` 为 SPA root，ACME 自签 HTTPS
4. 浏览器访问 https://simx.golia.jp 看见 simx logo + v1.0 badge + 27-tool 分组 + 3 个 CTA 链接到 GitHub README

## 前置条件

```bash
# v1.0 release on github
gh repo view goliajp/simx --json visibility -q '.visibility' | grep -q PUBLIC

# devops 通路
test -f ~/.config/devops/api-key
curl -sf -H "Authorization: Bearer $(cat ~/.config/devops/api-key)" https://devops.golia.jp/api/dns/zones >/dev/null

# git-flow 在 feature/web-scaffold 分支
test "$(git rev-parse --abbrev-ref HEAD)" = "feature/web-scaffold"
```

## 步骤（线性，无分叉）

### S1. DNS 注册 simx.golia.jp → t01 ✅ done

**动作**：
- 通过 devops-server HTTP API PUT golia.jp zone，append `{name: "simx", cname: "t01.golia.jp."}`
- `devops dns sync golia.jp` 推 Cloudflare

**验证**：`dig +short A simx.golia.jp @1.1.1.1 | tail -1` → `18.179.107.143` (t01 IP)，与 admin.golia.jp 一致 ✓

### S2. git-flow feature 分支 ✅ done

**动作**：`git flow feature start web-scaffold`（基于 develop）

**验证**：`git rev-parse --abbrev-ref HEAD` → `feature/web-scaffold` ✓

### S3. 脚手架 + 改配置 ✅ done

**动作**：
- `cp -R ~/workspace/goliajp/devops/starters/web /Users/doracawl/workspace/goliajp/simx/web`
- 删 `src/views/{about,components,state,home.test}.tsx` + `src/api/`
- `vite.config.ts` `base: '/'`
- `main.tsx` 删 basename + 删 demo route imports
- `app.tsx` 简化 nav（删 Components/State/About，加 GitHub 外链）
- `index.html` title + meta description
- `package.json` name → `simx-web`

**验证**：`bun install` → 364 packages installed ✓

### S4. v0.1 占位单页 ✅ done

**动作**：`src/views/home.tsx` 重写——Hero + 3 CTA card + 27-tool badge + "docs/demo coming soon" 段

**验证**：`bun run build` → `dist/index.html 0.68 kB / dist/assets/index-*.css 106.63 kB / dist/assets/index-*.js 496.85 kB / built in 223ms` ✓

### S5. .devops/deploy.yml ✅ done

**动作**：`.devops/deploy.yml` runs-on t01，upload `web/dist/` → `/var/lib/simx-web/` + caddy SPA action

**验证**：文件在场 ✓

### S6. commit + PR + self-merge

**动作**：
- 在 feature/web-scaffold 上 `git add web/ .devops/ docs/plan-cold/web-v0.1.md docs/plan-hot.md`
- 单个 commit message：`feat(web): scaffold simx.golia.jp v0.1 — placeholder landing + Caddy deploy`
- `git push -u origin feature/web-scaffold`
- `gh pr create --base develop --title "..." --body "..."`
- self-merge（PR-only ruleset 通过 admin bypass，0 required approvals）
- `git checkout develop && git pull && git branch -d feature/web-scaffold`

**验证**：`git log --oneline develop | head -2` 看到新 commit + `gh pr view <#> --json state -q '.state'` → MERGED

### S7. devops deploy + HTTPS verify

**动作**：
- `cd /Users/doracawl/workspace/goliajp/simx && devops deploy simx`
- 等 caddy reload + ACME 拿证（首次 30-60s）

**验证**（必须全通）：
```bash
dig +short A simx.golia.jp @1.1.1.1 | tail -1                      # → 18.179.107.143
curl -sI https://simx.golia.jp | head -1                           # → HTTP/2 200
curl -s https://simx.golia.jp/_ANY_PATH | grep -q 'id="root"'      # SPA fallback 命中
curl -s https://simx.golia.jp/assets/ -o /dev/null -w "%{http_code}\n" | grep -E "^(200|403|404)"  # asset dir 路径可达
```

## Checkpoint C1 验收

S7 4 条验证命令全通 = C1 done。

## 完成后动作

1. `mv docs/plan-hot.md docs/plan-history/web-v0.1-c1-hot.md`
2. 由 main convo 决定下一段：(a) web v0.2 docs framework，或 (b) simx 主项目 v1.1（Watch mode / Cell L4），按用户优先级触发新 plan-hot 生成（CLAUDE.md §6 模板）
3. 告知用户 simx.golia.jp 上线
