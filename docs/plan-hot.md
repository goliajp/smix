# plan-hot — v2.1 到 C2:singleton 形态、旧 JSON 导入、sims 注册表

## 目标 checkpoint

C2:`smix-store` 能表达全部六个旧状态文件的形态;用户机器上既有的 `.smix/sims.json`
在首次运行时自动进入 store,且**不删原文件**;`SimRegistry` 从 store 读写,`.smix/sims.json`
不再被 smix 写入。升级的用户看不出区别 —— 除了并发写不再互相截断。

## 前置条件

```bash
cargo test -p smix-store    # C1 的 14 测仍绿
```

## 步骤(线性)

### S1. singleton 形态

**红**
- 文件:`crates/smix-store/tests/singleton.rs`
- 断言:
  - `store.singleton("subprocess-ring").put_json(&vec)` → `get_json` 取回等值
  - 未写过的 singleton `get_json` 得 `Ok(None)`,不是错误
  - key 形状是 `one:subprocess-ring`(`raw_keys()` 断言,与 C1 三前缀同等对待:磁盘契约)

**绿**
- 文件:`crates/smix-store/src/lib.rs`
- API:`pub fn singleton(&self, name: &'static str) -> Singleton<'_>`,含 `get_json/put_json/delete`
- 关键点:
  - 环形缓冲与计数对都是"整体读、整体写"的单值,**不是** `Namespace` 的一条记录;
    硬塞进 `Namespace` 会让 `list()` 返回一个假的 id
  - 上限裁剪(ring 的 128)是调用方语义,不进 store —— store 不该知道谁需要裁剪

### S2. 旧 JSON 一次性导入

**红**
- 文件:`crates/smix-store/tests/import.rs`
- 断言:
  - 给定一个真实形状的 `.smix/sims.json`(取自 `smix-simctl/tests/registry.rs` 的 fixture 文本),
    `import_legacy_json(&store, path, "sim")` 后 `store.sims().list()` 含其中每个 alias
  - **原文件仍在**(导入不删用户数据)
  - 再导入一次不重复、不报错(幂等)
  - store 里**已有**该 key 时不覆盖(用户已经在新版上写过的东西,不能被旧文件回填)
  - 损坏的 JSON → 具名错误,**不是**静默跳过

**绿**
- 文件:`crates/smix-store/src/import.rs`
- 关键点:
  - 不删原文件。迁移失败时用户还能退回旧版本 —— 删掉就是不可逆
  - "已有则不覆盖"是方向性的:store 是新真源,旧文件只填空缺

### S3. `SimRegistry` 迁到 store

**红**
- 文件:`crates/smix-simctl/tests/registry.rs`(改造既有测试,不新建平行套件)
- 断言:
  - 既有 fixture 写出 `.smix/sims.json` 后,`SimRegistry::load` 仍能解析出同样的 alias
    (证明导入路径在真实调用链上生效)
  - `register` 之后 **`.smix/sims.json` 的 mtime 不变**(smix 不再写它)
  - 两个 `SimRegistry` 并发 `register` 不同 alias,两个都活下来
    (旧的读-改-写会丢一个;这条是这次迁移要买到的东西)

**绿**
- 文件:`crates/smix-simctl/src/registry.rs`
- 关键点:
  - `register` 从"读整个文件 → 改 → 写整个文件"变成"写一个 key"
  - `discover` 仍向上找 `.smix/`,但找的是 store 目录
  - `main.rs:841` 的 `.ok()?`(把损坏读成不存在)改为传播具名错误

**重构**
- `main.rs` 的 `registry_path()` 与 `SMIX_SIMS_JSON` 覆盖:保留环境变量语义,
  指向 store root

## Checkpoint C2 验收

```bash
cargo test -p smix-store -p smix-simctl -p smix-cli
grep -rn 'sims.json' crates/ --include='*.rs' | grep -v 'import\|test' | wc -l
```
期望:
- 测试全绿
- 生产代码里写 `sims.json` 的地方为 **0**(只剩导入模块与测试引用)

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.1-c2-hot.md`
2. 生成新 `plan-hot.md`(到 C3:runner state 两个写入方合一 + capsule)
