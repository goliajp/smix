# plan-hot — v2.1.0 发布

## 目标 checkpoint

S:v2.1.0 四通道发布并**独立复核**到位(crates.io / npm / Maven Central / Swift tag),
plugin marketplace 可被 `/plugin marketplace add goliajp/smix` 读到。
**发布是用户拍板的动作**,本计划做到「按一下就发」为止,不代拍。

## 前置条件

```bash
git status --porcelain | wc -l          # 期望 0
gh run list --branch feature/v2.0 --limit 1 --json conclusion --jq '.[0].conclusion'   # 期望 success
python3 scripts/dev/fact-scan.py        # 坐标全 2.1.0
cargo test -p smix-cli --bin smix release_record   # 发布 DAG 与工作区一致
```

## 步骤（线性,3 步）

### S1. dry-run 验证发布链

**绿（实现）**
- `SMIX_SHIP_DRYRUN=1 scripts/release/ship.sh 2.1.0 --i-know-what-im-doing`
- 重点看**新增的 CLI 腿**:从 CI 产物取三 triple 的 `smix` / `smix-mcp` →
  暂存进子包 → `bun publish --dry-run`;以及 semver-checks 是否认可
  「`ask` 未变、新增 `ask_with_attachments`」为 additive
- 关键点:dry-run 红了就修,**不放宽闸门**

### S2. 设备闸门

**绿（实现）**
- corpus gate 20/20(这批动过命中判据与 visible 判据,必须真跑)
- 两条闭环 e2e:`v2.13-c3-standalone-loop-e2e.sh` 与 `v2.13-c8-plugin-loop-e2e.sh`
- Android 两道设备闸门(ship 自带)

### S3. 交还拍板

**绿（实现）**
- 把「已验证 / 未验证 / 没做」三栏摆出来,等用户一句话再实发
- 实发后逐通道独立复核(不采信 ship 自报),并把 marketplace 推到远端

## Checkpoint 验收

```bash
SMIX_SHIP_DRYRUN=1 bash scripts/release/ship.sh 2.1.0 --i-know-what-im-doing > /tmp/dry.log 2>&1; echo $?
bash scripts/dev/v2.13-c3-standalone-loop-e2e.sh > /tmp/c3.log 2>&1; echo $?; tail -1 /tmp/c3.log
bash scripts/dev/v2.13-c8-plugin-loop-e2e.sh > /tmp/c8.log 2>&1; echo $?; tail -1 /tmp/c8.log
```
期望：三条退出 0,后两条末行分别为 `C3-STANDALONE-PASS` / `C8-PLUGIN-LOOP-PASS`。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.1.0-ship-hot.md`
2. 发布后按 CLAUDE.md §6 决定下一段(v2.13 的九个 checkpoint 已闭)
