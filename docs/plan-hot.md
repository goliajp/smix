# plan-hot — v2.5 到 C1:两份「破坏性变更」清单必须是同一份

## 目标 checkpoint

**C1**:`docs/v2.md` 的破坏性变更表与 `CHANGELOG.md` `## [2.0.0]` 的 `### Breaking` 一一对应,
由闸门 `crates/smix-cli/src/release_record.rs` 每次运行重新对账;
v2.1–v2.4 引入的三条破坏性变更在两处都有;每条都能在删掉时让闸门变红。

## 前置条件

```bash
git status --short                       # 期望:空(v2.4 已提交)
test -f docs/plan-cold/v2.5-release-record.md   # 期望:成立
bash scripts/dev/preflight.sh            # 期望:preflight: clean
```

---

## 本段预先定死的三个口径(执行期不得再议)

### 口径 1 — 闸门判「两份一致」,不判「这条算不算破坏性」

「加一个 `pub` 字段算不算破坏」取决于结构体是不是 `non_exhaustive`、调用方是不是用字面量构造它。
**这是判断,不是可机械求值的事实。** 闸门若去判它,就会得出一个自己编的答案。

因此判据只有一条:**两份清单的条目集合必须相等**。谁进清单是人决定的;
一旦进了,两处都得有。这与 `audit-ledger-scan` 的定位同源 ——
它验「引文还成立」,不验「引文的意思与状态相符」,并且把这件事写在 docstring 里。

### 口径 2 — 连接键是**编号**,不是措辞

`v2.md` 的表有 `#` 列(1–6);CHANGELOG 的 `Breaking` 是无序列表。
两边措辞注定不同(一个是给自己看的中文计划,一个是给用户看的英文发布说明),
**拿文字相似度对账等于制造一个永远在抖的闸门**。

做法:CHANGELOG 每条 `Breaking` 结尾加一个不可见于渲染的锚 —— 不行,HTML 注释在
`hygiene-scan` 眼里是开发噪音,而且用户读源码时会看见。

**定死的做法**:`v2.md` 表加一列 `changelog`,取值是该条在 CHANGELOG `Breaking` 里
**第一个加粗短语**(`**…**` 之间那段,英文原文)。闸门:
- 表里每一行的 `changelog` 值必须在 CHANGELOG 的 `Breaking` 段里作为加粗短语出现
- CHANGELOG `Breaking` 段里每一个加粗短语必须被表里某一行引用
两向都查,于是任一边加条目而另一边不加 = 红。

**一个例外要预先处理**:CHANGELOG 现有 8 条里,有一条(`SMIX_*` escape-hatch env vars removed)
的加粗短语里含反引号与星号,抽取时要按「`**` 到下一个 `**`」逐字取,不做任何规整。

### 口径 3 — 三条新破坏性变更的措辞与归属,现在定

| # | v2.md 表述 | CHANGELOG 加粗短语 |
|---|---|---|
| 7 | `DeviceControl::launch_with_args` 加 `activity` 参数 | `The Android launch activity is resolved, not assumed` |
| 8 | `LaunchAppOptions` / `Flow` 加 `launch_activity` 字段 | 与 #7 同一条 —— 它们是一次改动的三个面 |

**合并成一条,不拆三条**:三处签名变化服务同一个能力(启动 Activity 不再是猜的),
用户读发布说明时要知道的是「这件事变了」,不是「三个符号各自变了」。
迁移说明写在同一条里。

另外两条**已在 CHANGELOG 而不在 v2.md 表**的(smix-server 去数据库、选择器映射拒未知键)
按现有 CHANGELOG 措辞回填进表,不重写。

于是 C1 结束时:表 **9 行**,CHANGELOG `Breaking` **9 条**。

---

## 步骤(线性,2 个)

### S1. 闸门先红

**红(写测试)**

- 文件:`crates/smix-cli/src/release_record.rs`(新)+ `main.rs` 加 `#[cfg(test)] mod release_record;`
- 两个断言臂:
  - `every_breaking_change_is_in_both_lists` —— 口径 2 的双向对账
  - `the_breaking_table_shape_holds` —— 表必须有 `changelog` 列;行数下界 `>= 6`
    (少于既有六项 = 抽取失配,会因一无所知而通过)
- docstring 按本仓范式写全:**防的是哪一次事故**(两份清单 6 vs 8,且三条新变更两处都没有)
  + **WHAT THIS CANNOT SEE**(不判某条是否真的破坏性;不判 `Added`/`Fixed` 的覆盖度;
  不判迁移说明是否可行)
- 跑:`cargo test -p smix-cli --bin smix release_record`,应看到红,
  失败文本点名**两边各自多出来的条目**

**绿(实现)**

- 文件:`docs/v2.md` —— 表加 `changelog` 列;补第 7 行(口径 3)与两条回填行
- 文件:`CHANGELOG.md` —— `### Breaking` 补一条 `**The Android launch activity is resolved,
  not assumed**`,内容含:三处签名变化、`activity:` 覆盖仍生效、`.MainActivity` 只作最后回退、
  以及**自己实现 `DeviceControl` 的调用方要改签名**这一句迁移说明
- 跑:全绿

**重构**

- 无。

### S2. 接线 + 验红

**红(写测试)**

- 文件:`crates/smix-cli/src/release_record.rs`
- 加自查臂 `this_gate_runs_where_it_must`,与 `guide_gate` 同形:
  preflight 的文档反查(`docs/v2.md` 与 `CHANGELOG.md` 都被 `include_str!` 进本 crate,
  所以只改这两份文件时 preflight 会把 smix-cli 拉进来 —— **这一条要实际验证**,
  不是假设:改一行 `CHANGELOG.md` 然后跑 preflight 的 crate 推导,确认 smix-cli 在列)
- **三次装回缺陷验红**,结果写进决策日志:
  1. 从 `v2.md` 表删一行 → 红,点名 CHANGELOG 里那个孤立短语
  2. 从 CHANGELOG 删一条 → 红,点名表里那个悬空引用
  3. 把表里某行的 `changelog` 值改一个字 → 红

**绿(实现)**

- 文件:`docs/v2.md` 决策日志按 §10 追加一行,写明口径 1(闸门不判什么)与三次验红结果
- 跑:`bash scripts/dev/preflight.sh`

**重构**

- 无。

---

## Checkpoint C1 验收

```bash
cargo test -p smix-cli --bin smix release_record -- --nocapture 2>&1 | grep -E 'release-record:|test result:'
grep -c '^| [0-9] |' docs/v2.md
awk '/^### Breaking/,/^### Added/' CHANGELOG.md | grep -c '^- '
bash scripts/dev/preflight.sh
```

期望:

1. 一行 `release-record: 9 breaking changes, both lists agree`;且 `test result: ok. … 0 failed`
2. 第二条输出 `9`
3. 第三条输出 `9`
4. 第四条最后一行 `preflight: clean`

## 完成后动作

1. `mv docs/plan-hot.md docs/plan-history/v2.5-c1-hot.md`
2. 生成新 `docs/plan-hot.md`(覆盖 C2:CHANGELOG 的 `2.0.0` 段覆盖 v2.1–v2.4 实际做过的事),
   附加专属 context:
   - **覆盖判据由 C1 的结果定**,冷计划已经写明这一点 —— 不要在 C2 起手时才发现没有判据
   - v2.1–v2.4 的用户可见改动清单在四段的决策日志里,不在别处
   - C2 **不**碰 `docs/scope-decisions-pending.md` 的三条:那是委托方的决定
