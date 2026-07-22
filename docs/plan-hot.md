# plan-hot — v2.5 到 C2:发布说明知道后四段做了什么

## 目标 checkpoint

**C2**:`docs/guide-executability.md` 的每一行,凡**改了行为**的,都能在 `CHANGELOG.md`
`## [2.0.0]` 里找到对应条目;由闸门 `release_record.rs` 双向对账,删任一边即红。

## 前置条件

```bash
git status --short                     # 期望:空(v2.5-C1 已提交)
cargo test -p smix-cli --bin smix release_record -- --nocapture 2>&1 | grep 'release-record:'
# 期望:9 breaking changes, both lists agree
awk '/^## \[2.0.0\]/,/^## \[1\./' CHANGELOG.md | grep -ci daemonProxy
# 期望:0 —— v2.3/v2.4 的行为改动一条都没进发布说明
bash scripts/dev/preflight.sh          # 期望:preflight: clean
```

---

## 本段预先定死的两个口径(执行期不得再议)

### 口径 1 — 复用 C1 的连接键机制,不发明第二种

C1 已经证明「加粗短语逐字」这个连接键可用:两边措辞天然不同,而短语是**同一个字符串**。

C2 用同一套:`docs/guide-executability.md` 加一列 `changelog`,取值是
`CHANGELOG.md` `## [2.0.0]` 段里某条的开头加粗短语,或 `—`。

**冷计划说 C2 的判据「等 C1 的结果出来再定」——这就是结果。** 不为 `Added` / `Fixed`
另造一份清单去对账:那会变成一份契约两个实现,正是前面几段一直在拆的东西。
现成的清单已经有了,就是那 8 行。

### 口径 2 — `—` 是「只改了文档」,而这一点闸门查得动

8 行里有的只改了页面(N3 把三处说反的机制改对),有的改了行为(N1 给 16 个子命令加
`--device`)。只改文档的不该进发布说明 —— 用户读不到「我们把一句话写对了」。

判据不靠人声明,靠**已有的 `层` 列**:
- `层` 是 `—`(`runs` 行都是)→ 看不出来,不能当判据

所以改用**这一行的 probe 名**做判据 —— 也不行,probe 名不说明改了什么。

**定死的做法**:`changelog` 列取 `—` 时,该行必须在**新增的 `kind` 列**里写 `docs`;
写 `behaviour` 的行必须给出真实短语。`kind` 是人填的一个二选一,闸门查的是
**一致性**(`docs` ⟺ `—`),不是「这一行到底改没改行为」——
后者与 C1 的「某条算不算破坏性」同类,是判断,写进 docstring 的「看不见什么」。

预先定死每一行的 `kind`,执行期不得改:

| 行 | kind | 理由 |
|---|---|---|
| N1 | behaviour | 16 个子命令新增 `--device`,端口解析链改了 |
| N2 | behaviour | 已在 `Breaking` 第 9 条 —— `changelog` 指向它 |
| N3 | docs | 只把页面里说反的三处改对,代码一行没动 |
| N5 | behaviour | 页面改了,但同时补了 `arrow_up` 等四个拼法 |
| N6 | behaviour | 表达式文法多一层 |
| N7 | behaviour | `text:` 接受 tagged 形态 |
| P1 | behaviour | tap 三路由认 id / label(v2.3-C5) |
| P2 | behaviour | 裸字符串形态搜全部可读串(v2.3-C5) |

于是 C2 要往 CHANGELOG 补的是 **6 条**(N2 已在 Breaking,N3 不进)。

---

## 步骤(线性,2 个)

### S1. 闸门先红

**红(写测试)**

- 文件:`crates/smix-cli/src/release_record.rs`
- 新臂 `every_behaviour_change_reaches_the_release_notes`:
  - 读 `docs/guide-executability.md` 的行(格数从 9 变 11 —— 加 `kind` 与 `changelog`)
  - `kind` ∈ `{docs, behaviour}`;`docs` ⟺ `changelog == "—"`
  - `behaviour` 行的 `changelog` 必须是 `## [2.0.0]` 段里**某条的开头加粗短语**
    (`Breaking` / `Added` / `Fixed` 三节都算 —— 用户不在乎它被归到哪一节)
  - 反空转下界:`behaviour` 行 `>= 5`
- **注意**:`guide_gate.rs` 的 `the_list_and_the_probes_agree` 也在读这张表,格数一变它会红。
  两处的格数检查要一起改,**不要只改一处让另一处静默跳过** ——
  那正是 `audit-ledger.md` 曾经漏掉一整行的形态
- 跑:红,点名 6 行没有对应条目

**绿(实现)**

- 文件:`docs/guide-executability.md` —— 加两列,按口径 2 的表填
- 文件:`CHANGELOG.md` `## [2.0.0]` —— 补 6 条:
  - `### Added`:`**Single-shot verbs take `--device`**`(N1)、
    `**Relational operators in `assertTrue`**`(N6)、
    `**An explicit regex form for `text:`**`(N7)
  - `### Fixed`:`**The tap routes resolve id and label**`(P1)、
    `**`smix authoring suggest` searches every readable string**`(P2)、
    `**Underscored arrow key names**`(N5)
  - 每条写清**用户之前撞到的是什么** —— 与该节现有条目同一文风(先说坏在哪,再说为什么)
- 跑:全绿

### S2. 验红 + 收口

**红(写测试)**

- 三次装回缺陷:
  1. 把某行 `kind` 从 `behaviour` 改 `docs` 而 `changelog` 仍指短语 → 红
  2. 删 CHANGELOG 里某条 → 红,点名悬空引用
  3. 把某行 `changelog` 改一个字 → 红

**绿(实现)**

- `docs/v2.md` 决策日志按 §10 追加一行:口径 1(为什么不另造清单)、口径 2(`kind` 是人填的、
  闸门只查一致性)、三次验红结果
- 跑:`bash scripts/dev/preflight.sh`

---

## Checkpoint C2 验收

```bash
cargo test -p smix-cli --bin smix release_record -- --nocapture 2>&1 | grep -E 'release-record:|test result:'
grep -c '| behaviour |' docs/guide-executability.md
bash scripts/dev/preflight.sh
```

期望:

1. 摘要含 `9 breaking changes, both lists agree` 与
   `7 behaviour changes in the release notes`;`test result: ok. … 0 failed`
2. 第二条输出 `7`
3. 第三条最后一行 `preflight: clean`

## 完成后动作

1. `mv docs/plan-hot.md docs/plan-history/v2.5-c2-hot.md`
2. v2.5 冷计划的 C3 是条件性的 —— 只有 C1/C2 翻出「发布前必须回答的问题」才热化。
   若无,回 `docs/roadmap.md` 与 `docs/v2.md` 确认 v2 是否只剩
   `docs/scope-decisions-pending.md` 那三条待拍板;若是,**停下来报给用户**,
   不自行推进:那是委托方的决定(§13)
