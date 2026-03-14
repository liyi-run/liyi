<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# 重构路线图

本文档列出了 linter crate（`crates/liyi`）中具体、低风险的重构机会。
每项重构都限定为一个独立的逻辑变更，可作为单独的提交落地。

各项按阶段分组。阶段内的顺序仅为建议而非硬性约束——任何一项均可独立
进行。

---

## 阶段 1 — 提取共享基础设施

小型、自包含的提取，后续阶段以此为基础。

### 1.1  共享哈希验证正则表达式

`Regex::new(r"^sha256:[0-9a-f]+$").unwrap()` 在 `sidecar.rs` 和 `check.rs`
中各自在运行时编译了一次。将其提取为 `hashing.rs` 中的单个
`LazyLock<Regex>`（或纯函数 `fn is_valid_hash(s: &str) -> bool`），在所有
调用处复用。

**涉及文件**：`hashing.rs`、`sidecar.rs`、`check.rs`

### 1.2  `walk_git_history` 辅助函数

`git_log_revisions` / `git_show` 的循环模式在 `approve.rs` 中出现了五次。
提取一个泛型高阶函数：

```rust
fn walk_git_history<T>(
    root: &Path,
    rel: &str,
    max: usize,
    f: impl FnMut(&str) -> Option<T>,
) -> Option<T>
```

每个现有调用点将简化为一行代码。

**涉及文件**：`git.rs`（新增辅助函数）、`approve.rs`（调用点简化）

---

## 阶段 2 — `check.rs` 参数打包

`check.rs` 中有四个函数抑制了 `clippy::too_many_arguments`，每个函数
接受 8–10 个参数。本阶段将共同参数打包为上下文结构体并加以传播。

### 2.1  引入 `ItemCheckCtx`

定义一个结构体，承载在 `check_item_hash`、`handle_hash_mismatch`、
`handle_past_eof` 和 `handle_tree_path_resolved` 之间传递的共享只读
上下文：

```rust
struct ItemCheckCtx<'a> {
    file: &'a Path,
    source_content: &'a str,
    source_markers: &'a [SourceMarker],
    fix: bool,
}
```

### 2.2  将调用点迁移至 `ItemCheckCtx`

用 `&ItemCheckCtx` 替换四个函数中展开的参数列表，移除所有
`#[allow(clippy::too_many_arguments)]`。

**涉及文件**：`check.rs`

---

## 阶段 3 — 统一跨度恢复逻辑

tree-path → 兄弟扫描 → 偏移启发式的级联恢复逻辑在 `check.rs` 和
`reanchor.rs` 之间存在重复。

### 3.1  提取 `recover_item_span`

创建一个共享函数（例如放在新的 `recovery.rs` 中或作为 `ItemSpec` 的
方法），封装级联恢复逻辑：

1. 尝试 `resolve_tree_path`。
2. 哈希不匹配时，尝试 `resolve_tree_path_sibling_scan`。
3. 回退至偏移启发式。

返回一个描述结果的枚举（未变更 / 已偏移 / 失败）。

**涉及文件**：新建 `recovery.rs`、`check.rs`、`reanchor.rs`

### 3.2  将 `check.rs` 和 `reanchor.rs` 接入共享辅助函数

用对 `recover_item_span` 的调用替换两个模块中的内联恢复逻辑。这将消除
约 100 行重复代码，并保证两条代码路径始终保持同步。

**涉及文件**：`check.rs`、`reanchor.rs`

---

## 阶段 4 — 诊断构造辅助方法

`Diagnostic { … }` 结构体字面量包含 10 个字段，在 `check.rs` 中重复了
约 30 次。

### 4.1  添加 `Diagnostic` 构造方法

在 `Diagnostic` 上引入一组命名构造函数（例如 `Diagnostic::current(…)`、
`Diagnostic::stale(…)`、`Diagnostic::shifted(…)`），填充模板字段。

**涉及文件**：`diagnostics.rs`、`check.rs`

---

## 阶段 5 — `approve.rs` 收集逻辑拆分

`collect_approval_candidates` 约 195 行，包含三个独立的收集块
（未审查 / 失效已审查 / 需求变更）。

### 5.1  拆分为专注的收集函数

提取 `collect_unreviewed`、`collect_stale`、`collect_req_changed` 为
独立函数。`collect_approval_candidates` 变为一个合并结果的薄编排层。

**涉及文件**：`approve.rs`

---

## 阶段 6 — `tree_path/` 语言配置宏

每个 `lang_*.rs` 文件都定义了一个几乎相同的 `LanguageConfig` 静态量。

### 6.1  声明 `declare_language!` 宏

编写一个声明式宏，从简洁的 DSL 生成 `LanguageConfig` 静态量，强制结构
一致性，将每种语言的样板代码从约 50 行缩减到约 10 行。

### 6.2  迁移现有语言配置

将每个 `lang_*.rs` 转换为使用该宏。

**涉及文件**：`tree_path/mod.rs`、所有 `tree_path/lang_*.rs`

---

## 非目标

- **不引入行为变更。** 以上每项均为纯重构——测试在变更前后必须完全通过。
- **不引入新依赖**，`LazyLock`（自 Rust 1.80 起已稳定）除外。
- **不进行投机性抽象。** 每项提取均由现有重复代码驱动，而非假设性的未来
  需求。
