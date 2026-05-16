# simx Claude Code plugin — 安装与验证

> v1.0 release 落地形态：`.claude-plugin/plugin.json` + 嵌入式 `mcpServers.simx`。
> 本地 dev 流程已 v1.0 release ready；Marketplace publish v1.0+ roadmap。

## 1. 前置要求

1. macOS ≥ 15（与 CI `runs-on: macos-15` 同步）
2. Xcode ≥ 26.0 + iOS 26.x runtime（`xcodebuild -version` + `xcrun simctl list runtimes -j`）
3. `bun --version` ≥ 1.1.0（`package.json.engines.bun` 字面）
4. `claude` CLI 已安装并登录（`claude -p ping` 应返非空文本；与 `simx doctor` 字面对齐）

环境自检：

```bash
bun src/cli/index.ts doctor --json | jq '.compatibility.status'
# 期望: "supported"
```

## 2. 本地 dev 安装

把仓库当前路径作为 plugin root 传给 `claude` CLI：

```bash
claude --plugin-dir /absolute/path/to/lab15-autofix
```

执行后：

- `claude` 会读取 `.claude-plugin/plugin.json` 字面
- 嵌入的 `mcpServers.simx` entry 字面注册为 MCP server
- `${CLAUDE_PLUGIN_ROOT}` 字面被 plugin loader 端展开为上面传入的绝对路径

> 改 plugin 后需重启 `claude` session（loader 不热重载 manifest）。

## 3. 验证

四步逐条过：

1. 在 `claude` 交互式 session 内输入 `/mcp`：
   - 应见 `simx` server 字面 + status `connected`
   - 应见 **27 tools**（v0.6 C6 baseline：`ping` + 7 lifecycle + 4 observe + 7 interaction + 3 compound + 4 system + 1 vlm `explain_screen`）
   - 此步交互式 stdout 不 pipe-able，靠人工目视
2. 本地 MCP server 字面可启（与 `simx-c1-mcp-smoke.sh` 同源路径）：
   ```bash
   bun src/cli/index.ts mcp < /dev/null
   # 期望: 立即退出 0 / stderr 含 [simx-mcp] 启动日志
   ```
3. 结构契约 gate 字面通过：
   ```bash
   bash scripts/simx-c5-plugin-validate.sh | jq -e '.all_ok == "ok"'
   # 期望: 11 字段全 "ok" / exit 0
   ```
4. doctor 6 check 全 supported（C4 字面延续）：
   ```bash
   bun src/cli/index.ts doctor --json | jq -e '.compatibility.status == "supported"'
   ```

## 4. Marketplace publish（v1.0+）

placeholder —— 该路径推 v1.0 release 时一次性落地：

- 经 Claude Code plugin marketplace 提交 manifest
- session 内 `/plugin install simx` 一键启用
- 当前 `homepage` / `repository` 字段字面 `https://github.com/goliajp/simx` 是占位 URL，v1.0 发布前替换为真 repo
- 当前 v0.7 C5 范围内**不**含 marketplace publish 脚本；C7 README v1.0 rewrite 时一次性补完
