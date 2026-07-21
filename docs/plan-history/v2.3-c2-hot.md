# plan-hot — v2.3 到 C2:两处自指缺口 —— 自称守发布的闸门不在发布脚本里,自称迁移路径的工具够不到它承诺的东西

## 目标 checkpoint

C2:发布路径与范围文件里**两句关于自己的假话**被消灭,且其中可机器导出的那一句**从此有闸门盯着**。

做完的样子:

- `scripts/release/ship.sh` 真的调用 `hygiene-scan.py`(以及探测中发现的第二个同类缺口 `workflow-scan.py`),
  两段都**非 bypassable**。
- `scripts/dev/workflow-scan.py` 新增**检查 6**:preflight 的 source-gate 列表从磁盘导出,
  每个 gate 必须被 CI 与 ship **真调用**(注释里提到不算)。它在实现的当场就是红的 —— 点名两个缺口。
- `docs/v2.md` 「六项破坏性变更」表的「迁移」列改成属实的说法:表里**恰好一行**提 `codemod`,
  且是 #6;#1/#3/#4/#5 各自写清**消费者手工要做什么、去哪儿看**。
- 产品代码零改动 —— 机器判定(见验收第 7 条)。

**这一段要消灭的东西**:C1 治的是「一份清单写下时为真、后来为假、没人盯」。C2 治的是同一物种的另一副面孔 ——
**写下时就没核对过**,而它自称的身份(「发布闸门」/「迁移路径」)让读者以为核对过了。
`hygiene-scan` 的 docstring 第 4 行写着 "so it can gate a release",而 ship.sh 从头到尾只在**注释里**提过它两次。
`v2.md` 的破坏性变更表把一个 yaml codemod 列为三项 Rust/SDK 层变更的迁移路径 —— 那张表写于 2026-07-14 的计划期,
codemod 2026-07-17 才成形,**没有人回头看过它是否兑现**。

---

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix

git status --short                      # 期望:只有 `?? docs/plan-hot.md`(本文件)
test ! -f docs/plan-cold/v2.3-release-truth.md && echo MISSING || echo ok   # 期望:ok
pgrep -fl 'cargo|xcodebuild|gradle'     # 进 S1 前看用户是否在编译
bash scripts/dev/preflight.sh           # 期望末行:preflight: clean
```

**热化时已完成的本机探测(下面按它写,不按冷计划的假设写)**:

- `python3 scripts/dev/hygiene-scan.py` **exit=0**,末行 `hygiene-scan: clean — no development noise, no dead doc pointers`。
  即它当前是绿的,接进 ship **预期不会红**。口径仍在 S2 预先定死,不靠这个预期。
- `grep -n 'hygiene' scripts/release/ship.sh` 只命中**第 192 / 195 行,两处都是注释**
  (`# hygiene-scan asks "does it read as internal?"; fact-scan asks "is it true?"`)。
  **注释提到 ≠ 调用** —— 这一条是承重的,检查 6 必须先剥注释再匹配,否则它自己就会被这两行骗过去。
- **冷计划漏了一个同类缺口**:`workflow-scan.py` **也不在 ship.sh 里**
  (`grep -c 'workflow-scan' scripts/release/ship.sh` = 0)。preflight 第 81 行的 source-gate 列表有 6 个 gate,
  ship 只跑其中 4 个。所以这不是一个孤例,是**两个实例的物种** —— 这直接决定了口径三的答案(见 S1)。
- preflight 第 81 行:`for gate in hygiene-scan route-conformance fact-scan workflow-scan android-gate-scan audit-ledger-scan; do`,
  第 82 行 `python3 "scripts/dev/$gate.py"`。**preflight 里字面量 `scripts/dev/<name>.py` 并不出现**(由循环变量拼出),
  所以检查 6 对三处必须用**各自的调用形态**匹配,不能一套正则打天下。
- `.github/workflows/ci.yml` 的 `source-gates` job 六个 gate **全在**(第 80-91 行),外加 `gen-llms.py --check`。
  即缺口只在 ship 一侧。
- `crates/smix-migrate/` 全目录:`SMIX_` **0 处**、`Modifier` 0 处、`session` 0 处、`recorder_ir` 0 处。
  **冷计划热化 brief 里「codemod 源里 `SMIX_` 9 处」一说与实测不符** —— 见 S3 的逐格证据表,#3 的口径按实测写。
- `smix migrate --help` 自述:`Static maestro → smix yaml codemod.` 规则表由
  `crates/smix-migrate/src/lib.rs` 的 `default_rules()` 从 `smix_verbs::VERB_TABLE` 导出 —— **只改 yaml 的 verb 名与参数键**。
- `CHANGELOG.md` 的 2.0.0 段**说的是对的**:"`smix migrate` rewrites v1 flow yaml"。错的只有 `docs/v2.md` 这一处内部范围文件。

---

## 步骤(线性,无分叉)

### S1. 防复发闸门先行:workflow-scan 检查 6 —— 每个 source gate 三处都要**真被调用**

**为什么闸门写在修复之前**:先修 ship.sh 再补闸门,闸门就会被写成"能让现在这份 ship.sh 过"的形状。
先让它对着现状变红、点名两个缺口,再去接线 —— 与 C1 的 S2 同一范式。

**口径三的答案(在此定死,执行期不再议)**:**建**。理由三条,都可验:

1. **它是可从磁盘导出的**。gate 集合来自 preflight 的列表 + `scripts/dev/*-scan.py` 的 glob,
   不是手工维护的名单 —— 与 `android-gate-scan` 从 gradle 模块导出期望值同一手法。
2. **它已经有两个实例**(`hygiene-scan` 与 `workflow-scan`),不是一次意外。
3. **`workflow-scan` 已有先例但覆盖不到**:它的检查 3 管 `scripts/dev/*-guard.sh` 必须被 hook 调用
   ——「存在但没接线的守卫,与正在工作的守卫无法区分」。python gate 是同一句话的另一半,
   只是"接线"的对象从 hook 换成三处发布路径。所以**加进 workflow-scan 作检查 6,不新建脚本**:
   同一个问题的两半分居两个脚本,失败消息会说不清是哪一层。
   (这与 C1「不把 audit-ledger 并进 hygiene-scan」不矛盾 —— 那是三个**不同**的问题,这是**同一个**问题。)

**红(缺口当前不可观察)**

```bash
python3 scripts/dev/workflow-scan.py; echo "exit=$?"
```

期望 **exit=0 / `workflow-scan: clean`** —— 两个 gate 不在 ship 里,而闸门层对此**一无所知**。
这就是红:被检的性质存在,检查它的东西是瞎的。

**绿(实现检查 6)**

- 文件:`scripts/dev/workflow-scan.py`
- docstring 的 `Checks:` 段追加第 6 条,按本仓惯例**写明它防的是哪一次事故**:
  `hygiene-scan` 的 docstring 自称 "so it can gate a release",而 ship.sh 只在两行注释里提过它;
  `workflow-scan` 自己也一样。preflight 与 CI 跑六个 source gate,ship 跑四个 ——
  **漏掉的那一处恰好是通向用户的那条路**(与检查 3 的 adb-guard 同一句话)。
- 实现形态(写死,执行期不再选):

  ```python
  SOURCE_GATE_LOOP = re.compile(r"^\s*for gate in (.+?);\s*do", re.M)
  MIN_SOURCE_GATES = 4
  PREFLIGHT = "scripts/dev/preflight.sh"
  DOWNSTREAM = (".github/workflows/ci.yml", "scripts/release/ship.sh")
  ```

  - **先剥注释再匹配**:对 `ci.yml` / `ship.sh`,把 `lstrip()` 后以 `#` 开头的行整行丢掉,再在剩下的正文里找
    字面量 `scripts/dev/<name>.py`。**这一条是整条检查的承重点** —— 不剥注释,ship.sh 第 192/195 行
    会让 `hygiene-scan` 当场"通过",闸门变成它自己要治的那个病。**把这句理由写进代码注释**,
    否则下一个人会把剥注释当多余的复杂度删掉。
  - **preflight 用另一种形态匹配**:名字必须出现在 `for gate in …` 那一行的词表里(字面量路径在那里不出现)。
  - **检查 6a(磁盘 → preflight)**:每个 `scripts/dev/*-scan.py` 的 basename 去掉 `.py` 后必须在该词表里。
    没有它,一个谁都不调的新 scan 会因为"不在 preflight 列表里"而整个逃出检查。
  - **检查 6b(preflight → CI + ship)**:词表里每个名字,必须在两个下游文件的**非注释正文**里各出现一次。
  - **下限**:词表解析出 `< MIN_SOURCE_GATES` 个名字,判为**列表解析坏了**而不是 gate 变少了,直接红。
    理由与 `MIN_ROWS` / `MIN_MODULES` 同一条:一个读出零个名字的正则会让后面每条检查空洞地通过。
- **实现完成后必须立刻是红的**,exit=1,失败消息点名恰好两条:
  `scripts/release/ship.sh does not invoke hygiene-scan` 与 `… workflow-scan`。
  **只红一条或红三条以上都说明检查写错了** —— 回到实现,不要往下走。

**它检查不到什么(写进 docstring —— 说不出自己不查什么的闸门会被读成全知)**

- 只验**名字出现在非注释正文里**,不验它真的被执行:一行 `[[ -n "$SKIP" ]] || python3 scripts/dev/x.py`
  照样算调用。要防这个得跑一遍 ship,而 ship 会发版。
- 只覆盖 preflight 词表里的 gate 与磁盘上的 `*-scan.py`。**循环之外**被调用的东西
  (preflight 第 90 行的 `gen-llms.py --check`、`*-guard.test.sh` 那一圈)**不在导出集合里**。
  它们各自另有归属:guard 由检查 3/4 管,`gen-llms --check` 三处都在(热化时已核)但**无人盯**。
  诚实记下这个洞,不假装覆盖。

**重构**

- 不动 `scripts/dev/audit-ledger-scan.py`。它的自指检查(`"audit-ledger-scan" not in read(gate)`)
  有同样的"注释即可满足"的弱点,但检查 6b 对它是**严格更强**的(要求非注释正文里的完整路径),
  已经把它兜住了。为同一性质写两份逻辑,下次改的时候只会改一份。
  这条判断进 §10 决策日志,别只留在这里。

---

### S2. 接线:两个缺口进 ship.sh,检查 6 转绿

**绿(接线)**

- `scripts/release/ship.sh` 新增两段,均**非 bypassable**(在 `--i-know-what-im-doing` 分支之外):

  | 段 | 位置 | 日志 |
  |---|---|---|
  | `--- hygiene scan ---` | 紧接 `--- fact scan ---` **之前**(第 191 行那段之上) | `/tmp/smix-ship-hygiene.log` |
  | `--- workflow scan ---` | 紧接 `--- fact scan ---` **之后** | `/tmp/smix-ship-workflow.log` |

- **hygiene 放在 fact 之前的理由**:ship.sh 第 192-195 行的注释本来就在拿两者对照
  (「hygiene 问『读起来像内部吗』,fact 问『是不是真的』」)。此前那段注释在解释一个**不跑的东西**;
  把它放到被对照者之前,注释从悬空变成属实。**注释原文不动** —— 它一直是对的,错的是缺的那一步。
- **workflow 放在 fact 之后的理由**:它问的是第三个问题 —— 「发布出去的这份树,治理契约还完整吗」。
  adb-guard 的前例(脚本进了仓、让它跑的那一行没进)正是发布面该拦的那类。段注释写这一句。
- 失败文案照本仓形态:`|| fail "<gate> FAILED — <一句话说清漏了什么> (see <log>)"`。

**hygiene-scan 在 ship 调用点变红时的处置(口径二,预先定死,执行期不得临时决定)**

热化时实测它 exit=0,所以预期不红。**若仍红**,按命中类型走,四选一,没有第五类:

| 命中类型 | 处置 | 判据 |
|---|---|---|
| **A. prose 噪声**(未被引号保护的版本-checkpoint 标签 / 内部计划段号 / in-house 消费者名 / CJK 片段) | **改被检对象**(改措辞) | 默认路径。命中文件在 `README.md` / `CHANGELOG.md` / `docs/ai-guide/` / `npm/` / 任一 crate 的 `README.md` 上时,**只能走 A** |
| **B. dead doc pointer**(死指针) | **改指针,永不豁免** | 与 C1 对 `POINTER_SKIP` 的裁决同源:死指针是真信号 |
| **C. 该"噪声"是文件的主题本身** | 才允许加 `EXCLUSIONS` 条目,**必须带理由字符串** | 仅限内部文件(计划 / 记账 / 宪法 / rule card)。判据机械化:命中路径若落在 A 行列的用户可见面上,**一律不许走 C** |
| **D. hygiene-scan 误判**(引号内的数据被当成散文) | **修 hygiene-scan 的引号剥离逻辑**,不豁免文件 | 命中串出现在引号内即属此类 |

**两条禁止(写进验收断言,不靠自觉)**:

- **禁止**给 ship 的 hygiene-scan 调用传 `--noise-only` 或任何降级 flag —— 那等于接一个残缺的闸门,
  是本段要消灭的物种的变体(「自称守发布」变成「守发布的一半,没说是哪一半」)。
- **禁止**把这两段放进 bypass 分支或加 `|| true`。

**红转绿的确认**

```bash
python3 scripts/dev/workflow-scan.py; echo "exit=$?"    # 期望 exit=0
python3 scripts/dev/hygiene-scan.py;  echo "exit=$?"    # 期望 exit=0
```

**红向注入(三次,每次看到红再还原;还原一律走 `cp` 备份,禁止 `git checkout <file>`)**

| # | 注入 | 期望的红 |
|---|---|---|
| R1 | 从 `ship.sh` 删掉 `--- hygiene scan ---` 那一段 | 检查 6b 点名 `hygiene-scan` 不被 `scripts/release/ship.sh` 调用 |
| R2 | 把 `ship.sh` 里的 `python3 "$ROOT/scripts/dev/workflow-scan.py"` 整行改成 `# python3 …`(注释掉) | **仍然红**,点名 `workflow-scan` —— 这一条证明剥注释真的生效,是三次里最承重的 |
| R3 | 把 preflight 第 81 行词表删到只剩 2 个名字 | 触发下限:判为词表解析坏了,而不是 gate 变少了 |

R2 是承重的:没有它,检查 6 可能只是在做子串匹配,而**ship.sh 第 192/195 行的注释恰好能满足子串匹配** ——
那正是这条检查存在的原因。

**重构**

- 不把 ship.sh 里六个 source gate 折成一个循环。ship 的每一段都带着"它防的是哪一次事故"的注释,
  折进循环这些注释就无处可放,而那些注释是本仓最有价值的部分。

---

### S3. `docs/v2.md` 六项破坏性变更表:迁移列改成属实的说法

**这张表为什么可以就地改**:它是 CLAUDE.md §0 第 [2] 层的**范围文件**,不是历史日志。
append-only 的是 §10 决策日志 —— 那里追加一条记录这次更正(§10 要求),表本身直接改对。
**这与 C1「v2.md 原文一字不动」不矛盾**:C1 不动的是 §10 里 2026-07-19 那条**历史记账**,
本段动的是 §29-38 行的**范围声明**。历史该是写下时的样子,范围该是现在的样子。

**红(缺口当前不可观察)**

```bash
# 表里现在有几行声称 codemod 是迁移路径
awk '/^## 六项破坏性变更/,/^## Checkpoint/' docs/v2.md | grep -c codemod
```

期望 **5**(热化时实测:#1 / #3 / #4 / #5 / #6 五行都含 `codemod`,其中 **#1 / #3 / #4 / #5 四行**
是 codemod 结构上够不到的东西;只有 #6 属实)。改完应为 **1**。

**逐格证据(执行期先跑证据命令,再落笔;命令的输出与下面的预期不符时,按输出写并在记账段记下分歧 —— 不问用户)**

| # | 变更 | 证据命令 | 迁移列改成 |
|---|---|---|---|
| 1 | sessions 强制 | `grep -rn 'fn driving' crates --include="*.rs"` → `crates/smix-sdk/src/lib.rs:817`;`grep -c 'session' crates/smix-migrate/src/lib.rs` → 0 | **yaml 无需改**(flow 自己的 `appId:` 开 session);**Rust / SDK 调用方手工改** —— 改调 `App::driving()`,详见 CHANGELOG 2.0.0 Breaking |
| 2 | wire schema 协商 | `grep -n 'negotiate_wire_schema' crates/smix-runner-wire/src/lib.rs` → 546 行 | **无需改** —— runner 仍答 schema 1,v1.x 客户端照常协商;握手在 `/health` |
| 3 | `SMIX_*` env 折进 config | `grep -c 'SMIX_' crates/smix-migrate/src/lib.rs` → **0**;`grep -n 'is deprecated; use .smix/config.yaml switches' crates/smix-cli/src/main.rs` → 1433 行 | **手工改**:env 搬进 `.smix/config.yaml` 的 `switches:` 块;旧 env 仍生效并按名 warn(`crates/smix-cli/src/main.rs:1433`)。**codemod 不碰 env**(它只改 yaml flow) |
| 4 | `Modifier(s)` + 双 `open_url` 合并 | `grep -rn 'pub struct Modifiers' crates --include="*.rs"` → `smix-selector/src/lib.rs:220`,单数 `Modifier` 已无;`grep -n '"openLink"' crates/smix-verbs/src/lib.rs` → 356 / 587 | **Rust / SDK 调用方手工改**到单一 `Modifiers` 模型;yaml 侧 `openLink` 的 verb 形态在 VERB_TABLE 里,由 `smix migrate` 覆盖 |
| 5 | crate rename | `grep -rn 'smix_recorder_ir\|smix-recorder-ir' crates` → **0 命中**(改名已完成) | **手工改**消费者自己的 `Cargo.toml` 依赖名与 `use` 路径(`smix_recorder_ir` → `smix_authoring_ir`) |
| 6 | VERB_TABLE freeze v2 | `smix migrate --help` 自述 static yaml codemod;`crates/smix-migrate/src/lib.rs` 的 `default_rules()` 由 `VERB_TABLE` 导出 | **保留 codemod 说法**:`smix migrate` 改 verb 名;它不认的 verb(`runScript` / `evalScript`)原样保留并 WARN |

**口径一的两条硬约束**:

- **不许把 #1/#3/#4/#5 写成「无迁移路径」就完事**。每一格必须回答消费者**具体做什么、去哪儿看** ——
  "手工改" 三个字后面必须跟着改哪儿(类型名 / 配置键 / import 路径)或指向 CHANGELOG 的 Breaking 段。
  一张说"你自己想办法"的迁移表,比一张说错话的更没用。
- **#3 按实测写,不按 brief 写**。热化 brief 称 codemod 源里 `SMIX_` 有 9 处,实测 `crates/smix-migrate/` 全目录 **0 处**。
  所以 #3 的「codemod 改写」那半**同样不成立**,只有「具名 warn」那半是真的 ——
  改后的表里 codemod 只剩 #6 一行。这是与冷计划 brief 的实证分歧,记进 §10。

**同步改一处引言**:`docs/v2.md` 第 17 行 in-scope #5 写着
「**六项破坏性变更** + `smix migrate` codemod(见下)」—— 这句话本身不假(codemod 确实存在且服务 #6),
**保持原文不动**;真正误导的是下面那张表的迁移列,改表即可。不为了对仗去动一句没错的话。

**绿(§10 决策日志追加)**

按 CLAUDE.md §10 格式追加一行,**行首标记写死为**:

```
- 2026-07-22 [v2.3-C2 破坏性变更表把 yaml codemod 列为 Rust/SDK 迁移路径 —— 改对] …
```

行首形态是承重的:验收第 7 条按 `^- 2026-07-22 \[v2.3-C2` 精确匹配。模糊 grep(只找 `C2`)会被
第 42 行「Checkpoint 概览」里的 `C2 AI 断言层` 骗过去 —— 那是 v2 的 C2,不是 v2.3 的 C2。
内容含:表被改对了哪几格、依据(codemod 由 `VERB_TABLE` 导出、只改 yaml,实测计数)、
以及**这张表没有闸门盯着**这个诚实交代(见下)。
**追加时注意**:正文若写到会被 sim-guard / adb-guard 拦的命令形状,heredoc 正文会被 guard 当命令读
(07-21 已发生过一次)—— 改措辞或改用编辑工具写入,**不改 guard**。

**为什么不给这张表新建闸门**(口径三的第二半,预先定死):

1. 它错的机制**不是**「写下时真、后来假」,而是**写下时就未经核对**(2026-07-14 计划期写,codemod 2026-07-17 才成形)。
   盯"后来变假"的闸门(C1 那种引文重求值)抓不到"一开始就假"。
2. 6 行的范围声明不是会漂的记账。要机器判「codemod 能不能够到这一项」,等于把每项破坏性变更的语义编码进闸门 ——
   那是把判断重写一遍,不是检查判断。
3. 唯一机械可判的那条不变量 —— **表里恰好一行提 codemod,且是 #6** —— 本段用**验收断言**钉住(见下第 4、5 条),
   不为一条断言新建一个脚本。**代价说清**:验收只在 checkpoint 时跑,这张表在 checkpoint 之间没有连续盯梢。
   这是承认的盲区,写进 §10,不是遗漏。

**重构**

- 不动 CHANGELOG。它说的是对的(`smix migrate` rewrites v1 flow yaml),改它等于把对的改成别的。

---

## Checkpoint C2 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix

# 1. 防复发闸门绿(检查 6 求值了六个 source gate 的三处接线)
python3 scripts/dev/workflow-scan.py

# 2. 两个缺口都真的进了 ship(非注释正文里)
grep -v '^\s*#' scripts/release/ship.sh | grep -q 'scripts/dev/hygiene-scan.py'
grep -v '^\s*#' scripts/release/ship.sh | grep -q 'scripts/dev/workflow-scan.py'

# 3. 接的是完整闸门,不是降级版
python3 scripts/dev/hygiene-scan.py
! grep -n 'hygiene-scan.py' scripts/release/ship.sh | grep -q -- '--noise-only'
! grep -n 'hygiene-scan.py\|workflow-scan.py' scripts/release/ship.sh | grep -q '|| true'

# 4. 破坏性变更表里 codemod 恰好一行
test "$(awk '/^## 六项破坏性变更/,/^## Checkpoint/' docs/v2.md | grep -c codemod)" = 1

# 5. 且那一行是 #6
awk '/^## 六项破坏性变更/,/^## Checkpoint/' docs/v2.md | grep '^| 6 |' | grep -q codemod

# 6. #1/#3/#4/#5 各自给出了消费者动作,不是空话
for n in 1 3 4 5; do
  awk '/^## 六项破坏性变更/,/^## Checkpoint/' docs/v2.md | grep "^| $n |" | grep -q '手工' \
    || { echo "row $n 的迁移列没说消费者做什么"; exit 1; }
done

# 7. §10 记了这次更正(行首标记写死,见 S3 —— 模糊 grep 会被「Checkpoint 概览」里的 C2 骗过去)
grep -q '^- 2026-07-22 \[v2.3-C2' docs/v2.md

# 8. 既有闸门没被本段破坏
python3 scripts/dev/audit-ledger-scan.py
python3 scripts/dev/fact-scan.py
python3 scripts/dev/route-conformance.py
python3 scripts/dev/android-gate-scan.py
bash scripts/dev/preflight.sh

# 9. C2 不碰产品代码
git status --porcelain -- crates swift-bridge android-runner npm web dashboard examples | wc -l | tr -d ' '
```

期望:

- 第 1 条 exit 0,末行 `workflow-scan: clean`
- 第 2、3、5、6、7 条 exit 0(`grep -q` 静默;第 3、6 条的 `!` / `||` 形式使失败可见)
- 第 4 条 exit 0(`test` 成立 = 表里 codemod 恰好一处)
- 第 8 条:四个闸门各自 exit 0,preflight 末行 `preflight: clean`
- 第 9 条输出 **`0`** —— 工作树里产品目录零改动。
  C2 的改动面是 `scripts/dev/workflow-scan.py` / `scripts/release/ship.sh` / `docs/v2.md` / `docs/plan-hot.md` 四个文件,
  产品目录不该出现在改动集里。**若第 9 条非 0**,说明处置口径二时走到了"改被检对象"而改进了产品代码 ——
  那是合法的(类型 A 命中就该改),但必须在收尾记账里逐文件写明改了什么、属于口径二的哪一类,**不允许口头带过**。

外加**已在 S1/S2 内完成并记录**的验证(它们要改工作树,不放进复跑命令):

- S1 绿相之后的**首次红**:检查 6 实现完成时 exit=1,恰好点名 `hygiene-scan` 与 `workflow-scan` 两条
- S2 的 R1–R3 三次注入各自变红一次,失败消息与表逐条对上;R2(注释掉调用行)必须仍红;还原走 `cp` 备份

---

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.3-c2-hot.md`
2. `docs/v2.md` 决策日志追加(§10),含四件事:
   - 破坏性变更表哪几格被改对了、依据是什么(codemod 由 `VERB_TABLE` 导出、只改 yaml;`crates/smix-migrate/` 里
     `SMIX_` / `Modifier` / `session` / `recorder_ir` **实测全 0**);
   - `workflow-scan` 检查 6 立起来了,**以及它查不到什么**(名字出现 ≠ 真被执行;循环外的调用不在导出集合里);
   - **`audit-ledger-scan` 的自指检查未加固**,理由是检查 6b 严格更强、已把它兜住 —— 记下来,免得下次当漏项重做;
   - **这张破坏性变更表没有连续闸门**,只在 checkpoint 验收时被断言钉一次 —— 承认的盲区,不是遗漏。
3. 按 §7 收尾 task 状态(S1/S2/S3 三个 task 全 `completed`)。
4. **不自行热化 C3**(§6):把 C2 的结论(尤其是「冷计划只点名了 hygiene-scan,实际是两个实例」这一条)
   报给用户,由用户说"开始 C3"。
