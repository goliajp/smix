# plan-hot — v2 到 C14-pre:发布链路上还有多少「声称有闸门、其实没有」

## 目标 checkpoint

**C14-pre**:v2.0.0 发布前把 `ship.sh` 全链跑一遍(**到发布为止,不发布**),并且把发布链路上
**每一条"某某会检查它"的声称**逐条对照代码核实。

今天的发现是这个 checkpoint 存在的理由:`build-runner-tarball.sh` 的头部写着
「pre-publish ship gate 调用它并比对 SHA256」,**那个 gate 不存在**,于是
`HitChain.swift` / `TouchTimeline.swift` 两个修复从未进过发给用户的 tarball,
而三条已经写进回信的「已做」在消费方机器上不成立。

`cargo publish` 不可撤销。**同一族的第二处,要在发布前找出来,不是发布后。**

## 前置条件

```bash
git status --short                      # 期望:空
ssh mini 'cd workspace/goliajp/smix && cargo test --workspace 2>&1 | grep -c "^test result: FAILED"'
# 期望:0
```

## 已经查清、不必重查的事实

- **v2.7 的 C1/C2/C3 + 追加的 C4 全部闭合**,EXT1 九条各有归宿
  (`.claude/dogfood/2026-07-22-ext1-response.md` 已改为实际结果)
- **发布清单是拓扑序的且有闸门**(`release_record.rs`:26 crates,今天跑过)
- **破坏性变更两侧对账有闸门**(12 条,两边一致)
- **runner 源新鲜度现在有闸门**(`tarball_is_current.rs`,今天新增,红向验过两次)

## 本段预先定死的口径

### 口径 — 找的是「声称」与「事实」的差,不是找 bug

要扫的**不是**代码缺陷,是**注释 / 文档 / 计划里写着"由 X 保证"而 X 不存在或够不到**的地方。
今天那条的形态是:脚本头部写着一个 gate,`grep` 调用方零命中。

同族可疑面(按发现成本排序,不是按重要性):

1. `scripts/**` 里所有形如「Called by / 由…保证 / gate 会…」的注释 → 对照真实调用方
2. `ship.sh` 每一步的失败是否**真的**阻断(有没有 `|| true` / 吞掉退出码的)
3. 四个 SDK 的版本齐步(`SDK lockstep`)靠什么保证,那个东西跑不跑
4. `docs/` 里对读者承诺的可执行命令,是否真能跑(`guide_gate` 只覆盖 yaml 块)

**扫到就补闸门,不是补注释** —— 把注释改成实话只是让它不再撒谎,不能阻止下一次漂移。

## 步骤(线性,2 个)

### S1. 扫「声称有闸门」

**红**
- 文件:`crates/smix-cli/src/release_record.rs`(既有闸门里加,不新起一处)
- 断言:`scripts/` 下每一条声称被某物调用的注释,那个调用方在仓库里 grep 得到

**绿**
- 对每条声称:调用方存在 → 留;不存在 → **补上真实的闸门**,再把注释改成实话
- 关键点:闸门要机械可判(读脚本文本 + grep 调用方),不靠人读

**重构**
- 无

### S2. 跑到发布为止

**红**
- 无新测试。这一步是执行既有闸门

**绿**
- 在 mini 上跑 `ship.sh` 到 publish 之前的每一步,逐条记录通过与否
- 关键点:**任何一步失败即停并如实记**,不绕过、不 `|| true`

## Checkpoint C14-pre 验收

```bash
ssh mini 'cd workspace/goliajp/smix && cargo test --workspace 2>&1 | grep -c "^test result: FAILED"'
cargo test -p smix-cli --bin smix release_record -- --nocapture
cargo test -p smix-cli --bin smix guide_gate -- --nocapture
cargo test -p smix-runner-sources --test tarball_is_current
```

期望:第一条输出 `0`;其余三条 exit 0。
外加:`docs/v2.md` 决策日志新增一条,列出扫出的每一条「声称 vs 事实」及其处置
(补了闸门 / 改了注释 / 确认属实),**一条都不省略**。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2-c14-pre-hot.md`
2. **发布本身要用户拍板** —— `cargo publish` / `bun publish` / `gradle publish` /
   `git tag` 全部不可撤销且对外,不在 autorun 范围内。把「全链已跑到发布为止,
   结果如下」交给用户,由用户决定发不发


---

## 归档记录(2026-07-23,C14-pre 通过)

**S1 与 S2 都闭,两步各自暴露了「声称 vs 事实」的真差。**

- **S1(扫声称有闸门)**:`workflow-scan.py` 已经把「每个 `*-scan.py` 进三门、每个 `*-guard.sh` 有 harness」实现了,但没推广到一般脚本。新加 `check_every_dev_script_runs`,一红就照出 `fence-check.sh` 运行于零处(一个归档 checkpoint 的验收块跑过一次,之后全程无人跑)。接进三门。**这个 check 自己绊了自己三次**(读自身 docstring / 字面三引号破坏配对 / 把枚举文件的 glob 读成调用),每一个都是它本该抓的「提到≠运行」的变体,全部修进工具并保留为教训。红向验过。
  - 另核实两条「声称」属实(非误报):`android-gate-scan` 三门都跑、`*-guard.test.sh` preflight 用 glob 循环真的逐个跑 —— 查了才没冤枉。
- **S2(跑 ship 到发布前)**:两道 gate 亮红,各揪一个真缺陷 —— `list-sessions` 每次 panic(嵌套 runtime)、TS `FailureCode` 硬编码 9。都修+补测。crates/npm/Maven/tag 全未执行,发布待用户。

细节见 `docs/v2.md` 决策日志 2026-07-23 三条(workflow-scan / list-sessions / TS 魔数)。
