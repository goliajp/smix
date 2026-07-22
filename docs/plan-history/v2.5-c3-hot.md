# plan-hot — v2.5 到 C3:发布脚本会在半路停下,而没有任何东西说过

## 目标 checkpoint

**C3**:`scripts/release/ship.sh` 的两处会在 ship 当天失败的地方闭合,
且**由闸门 `crates/smix-cli/src/release_record.rs` 静态判定**,不需要真的发一次布才知道。

## 前置条件

```bash
git status --short                     # 期望:空
bash scripts/dev/preflight.sh          # 期望:preflight: clean
cargo semver-checks check-release --workspace > /tmp/semver.log 2>&1; echo $?
# 期望:1,且日志末尾是 `smix-ai-tier not found in registry (crates.io)`
```

---

## 两条实测到的缺陷(本段起因)

### 缺陷 A —— `smix-store` 不在发布清单里,而两个要发布的 crate 依赖它

`ship.sh:291` 的 `CRATES=(…)` 是**手写的 25 个**;工作区有 **30 个**。
差的五个(`smix-core` / `smix-core-conformance` / `smix-ffi` / `smix-server` / `smix-store`)
**没有一个声明 `publish = false`**。其中:

```
smix-cli    -> smix-store  (req ^2.0.0)
smix-simctl -> smix-store  (req ^2.0.0)
```

`smix-store` 不在 crates.io 上,于是 `cargo publish -p smix-simctl` 会被 registry 拒绝
(依赖未发布)。**而它排在 DAG 第 17 位** —— 前面十六个已经发出去了,
**crates.io 的发布不可撤回**。

### 缺陷 B —— `cargo semver-checks --workspace` 直接中止,而注释说它容忍这种情况

三个 crate 在 crates.io 上没有基线(`smix-ai-tier` / `smix-authoring-ir` / `smix-store`)。
工具**不是跳过它们**,是 `error: failed to retrieve index of crate versions from registry`
并以 1 退出。`ship.sh:248` 的注释写着它「blind to … brand-new crates」——
**那是一句从没被跑过的断言**,实际行为是整个 ship 在发布前一步失败。

25 个有基线的 crate **全部报 `no semver update required`** —— 也就是说,
本段之前所有的 ABI 改动都落在没有基线的那三个里,或者被 major 版本号覆盖了。
这一点先记下来,C3 不据此下结论。

---

## 本段预先定死的三个口径(执行期不得再议)

### 口径 1 — 发布清单不再手写,但也不自动生成

自动生成会把「要不要发布这个 crate」这个**决定**变成一个副作用。
`smix-server` / `smix-ffi` / `smix-core` 不发布可能是有意的,我不知道,
**而现在没有任何地方记着这件事** —— 这才是根因。

做法:每个不发布的 crate 在自己的 `Cargo.toml` 里写 `publish = false`。
那是 cargo 自己的表达方式,`cargo publish` 会照它执行,不需要第二处清单。
清单仍然手写(顺序是拓扑序,人排的),但闸门查:

- 工作区里每个**没有** `publish = false` 的 crate,必须在清单里
- 清单里每个名字必须是工作区里真实存在的 crate
- 清单顺序必须是依赖 DAG 的合法拓扑序(被依赖者在前)

于是「新加一个 crate 忘了发布」这件事当场红,而「这个 crate 不发布」是一个**写下来的决定**。

### 口径 2 — 五个未列 crate 的处置,现在定

| crate | 处置 | 依据 |
|---|---|---|
| `smix-store` | **加进清单**,排在 `smix-simctl` 之前 | 两个要发布的 crate 依赖它,没得选 |
| `smix-server` | `publish = false` | 是可执行服务不是库;没有任何要发布的 crate 依赖它 |
| `smix-ffi` | `publish = false` | cdylib,四个 SDK 通过二进制分发消费,不走 crates.io |
| `smix-core` | `publish = false` | 无人依赖 |
| `smix-core-conformance` | `publish = false` | 测试套件 |

**判据是「有没有要发布的 crate 依赖它」**,不是我对每个 crate 用途的印象 ——
后者正是这周反复出错的那种依据。四个 `publish = false` 全部满足「零发布依赖方」,
已用 `cargo metadata` 核过。

### 口径 3 — semver 检查按「有没有基线」分流,并且说出跳过了谁

`--exclude <SPEC>` 存在。ship.sh 改为:先问 crates.io 每个 crate 在不在,
不在的用 `--exclude` 排除,**并 log 出排除了哪几个及原因**。

理由与本仓既有习惯同源(「no silent caps」):一个默默少查三个 crate 的 gate,
读起来跟查全了一模一样。

**不**把这条判据搬进闸门 —— 它要联网。闸门只查 ship.sh **写了这段分流**,
与 `this_gate_runs_where_it_must` 同形。

---

## 步骤(线性,2 个)

### S1. 发布清单与工作区对上

**红(写测试)**

- 文件:`crates/smix-cli/src/release_record.rs`
- 新臂 `the_publish_list_covers_everything_that_ships`:
  - 用 `include_str!` 读 `scripts/release/ship.sh`,抽 `CRATES=(…)` 的名字
  - 用 `include_str!` 读每个 crate 的 `Cargo.toml`?——**不行**,要遍历工作区。
    改为读工作区根 `Cargo.toml` 的 `members`,再对每个成员读它的 `Cargo.toml`
    (路径由成员名推出,`crates/<name>/Cargo.toml`;成员表里有非 `crates/` 的就断言失败,
    说明布局变了)
  - 三条断言:未标 `publish = false` 的成员必须在清单里;清单里的名字必须是成员;
    清单顺序必须是拓扑序(用各自 `Cargo.toml` 里的 `smix-*` 依赖推)
  - 反空转下界:成员数 `>= 25`
- 跑:红,点名 `smix-store` 等五个

**绿(实现)**

- 四个 crate 的 `Cargo.toml` 加 `publish = false`,每处一行注释写明**判据**
  (「没有要发布的 crate 依赖它」),不写用途印象
- `ship.sh` 的 `CRATES=(…)` 加 `smix-store`,排在 `smix-simctl` 之前
- 跑:全绿

### S2. semver 检查不再在新 crate 上中止

**红(写测试)**

- 文件:`crates/smix-cli/src/release_record.rs`
- 新臂 `the_semver_gate_skips_crates_with_no_baseline`:ship.sh 里那段必须出现
  `--exclude` 与「log 出被排除者」的痕迹(查具体字符串,不查语义)
- 跑:红

**绿(实现)**

- `ship.sh` 的 semver 段:先探每个 crate 在不在 crates.io,不在的收进 `--exclude`,
  log 一行 `semver-checks: skipping N crates with no published baseline: …`
- 注释改真:删掉「blind to brand-new crates」那句(它描述的是工具不具备的行为),
  换成这段分流为什么存在
- **实跑一次** `bash -n` 语法检查 + 手动跑该段,确认 exit 0
- 跑:`bash scripts/dev/preflight.sh`

---

## Checkpoint C3 验收

```bash
cargo test -p smix-cli --bin smix release_record -- --nocapture 2>&1 | grep -E 'release-record:|test result:'
cargo semver-checks check-release --workspace $(python3 -c "…") ; echo $?   # C3 内定的等价入口
bash scripts/dev/preflight.sh
```

期望:

1. `test result: ok. … 0 failed`,摘要多一行 `publish list: N crates, topological`
2. semver 检查以 **0** 退出
3. `preflight: clean`

## 完成后动作

1. `mv docs/plan-hot.md docs/plan-history/v2.5-c3-hot.md`
2. v2.5 出口验收成立 → v2 只剩 `docs/scope-decisions-pending.md` 三条待拍板。
   **停下来报给用户**,不自行推进(§13)。
