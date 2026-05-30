<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Throwaway Roadmap: LSP Library Refactor

**Status:** temporary planning note, intended to be deleted or folded into
`docs/next-steps.md` / `docs/lsp-design.md` after the refactor lands.

**Target backlog items:** `docs/next-steps.md` 2.3 and 2.4 — expose a
cacheable requirement registry API before starting the LSP crate.

## English

### Current code shape

`crates/liyi/src/check.rs` already has a useful internal seam:

- `run_check()` discovers the repo, builds requirement state, checks each
  sidecar, emits post-pass diagnostics, sorts diagnostics, and computes the
  exit code.
- Pass 1 is split into private helpers:
  - `discover_requirements()`
  - `compute_requirement_hashes()`
  - `collect_source_related_refs()`
  - `enrich_requirements_from_sidecars()`
  - `detect_requirement_cycles()`
- Pass 2 is still exposed only as the private `check_sidecar()` helper.
- The registry state is still the private
  `HashMap<String, RequirementRecord>` plus the related side collections:
  `requirements_with_sidecar`, `requirements_referenced`,
  `source_related_refs`, and the requirement dependency graph.
- `RequirementRecord` is private and stores only the data currently needed by
  `check.rs`: source file, source line, stored sidecar hash, and freshly
  computed source hash.

The important observation: this is less of a large refactor and more of a
public API extraction around already-separated helper functions.

### Proposed public API shape

Add public types in `liyi::check` first, without changing CLI behavior:

```rust
pub struct RequirementRegistry {
    requirements: HashMap<String, RequirementRecord>,
    requirements_with_sidecar: HashSet<String>,
    requirements_referenced: HashSet<String>,
    source_related_refs: HashSet<String>,
    requirement_cycles: Vec<Vec<String>>,
}

pub struct RequirementRecord {
    pub file: PathBuf,
    pub line: usize,
    pub hash: Option<String>,
    pub computed_hash: Option<String>,
}
```

Then expose two functions:

```rust
pub fn build_requirement_registry(
    root: &Path,
    all_files: &[PathBuf],
    sidecars: &[SidecarEntry],
    source_cache: &mut HashMap<PathBuf, String>,
) -> (RequirementRegistry, Vec<Diagnostic>);

pub fn check_sidecars(
    root: &Path,
    sidecars: &[SidecarEntry],
    registry: &RequirementRegistry,
    source_cache: &mut HashMap<PathBuf, String>,
    fix: bool,
    dry_run: bool,
) -> Vec<Diagnostic>;
```

`run_check()` remains the CLI convenience wrapper. It should call `discover()`,
convert discovery warnings to diagnostics, call `build_requirement_registry()`,
call `check_sidecars()`, append post-pass diagnostics from the registry, sort,
and compute the existing exit code.

### Step-by-step implementation plan

1. **Make the data shape explicit.**
   - Rename the private `RequirementRecord` in place to a public struct.
   - Add `RequirementRegistry` as a wrapper around the existing pass-1 data.
   - Keep fields private at first unless tests or LSP call sites need direct
     access; prefer accessors for future API flexibility.

2. **Extract pass 1.**
   - Move the existing pass-1 calls from `run_check()` into
     `build_requirement_registry()`.
   - Return duplicate-requirement diagnostics from discovery through the
     function's diagnostic vector.
   - Store cycles in `RequirementRegistry`; do not emit cycle diagnostics yet
     if post-pass emission needs the final registry context.

3. **Extract pass 2.**
   - Rename the private `check_sidecar()` helper to avoid a public/private name
     collision, for example `check_one_sidecar()`.
   - Add public `check_sidecars()` that loops over sidecars and calls the
     one-sidecar helper with `registry.requirements`.
   - Keep writeback semantics exactly as they are: `fix && !dry_run` writes
     sidecars, `dry_run` never writes.

4. **Move post-pass registry diagnostics behind methods or helpers.**
   - Keep `emit_untracked_requirements()`, `emit_unreferenced_requirements()`,
     and `emit_cycle_diagnostics()` behavior unchanged.
   - Either call them from `run_check()` with registry fields, or provide a
     small internal helper such as `emit_registry_diagnostics()`.
   - Do not make the LSP API responsible for computing CLI exit codes.

5. **Preserve behavior with tests before adding LSP code.**
   - Existing golden tests should pass without output changes.
   - Add at least one focused library test if needed to prove
     `build_requirement_registry()` plus `check_sidecars()` gives the same
     diagnostics as `run_check()` for a fixture with related requirements.

### API design notes

- `build_requirement_registry()` should accept discovered `all_files` and
  `sidecars` rather than run discovery itself. The LSP will maintain its own
  discovery state and needs to rebuild the registry from cached file sets.
- The public registry should not expose mutation hooks yet. The first LSP can
  rebuild conservatively; incremental mutation can come after the API has real
  call sites.
- `source_cache` should remain caller-owned. This matches the LSP design, where
  the workspace state owns cached source text.
- Do not move approval-specific requirement registry code from
  `crates/liyi/src/approve.rs` in this refactor. It has different display needs
  for previous requirement text and can be unified later.

### Risks

- **Accidental behavior drift:** keep `run_check()` as the test oracle and avoid
  changing diagnostics, sorting, or exit-code logic.
- **Over-exposing internals:** start with a minimal registry wrapper and add
  read accessors only when the LSP needs them.
- **Fix-mode side effects:** preserve the current order of `--fix` operations
  so sidecar writes and `_hints` stripping remain unchanged.
- **Name collision:** introduce a public `check_sidecars()` wrapper rather than
  exposing the current mutating one-sidecar helper directly.

### Done criteria for the refactor commit

- `RequirementRegistry` and `RequirementRecord` are public from `liyi::check`.
- `build_requirement_registry()` is public and reusable by an LSP workspace.
- `check_sidecars()` is public and accepts a prebuilt registry.
- `run_check()` remains source-compatible and behavior-compatible for the CLI.
- `cargo fmt --check`, `cargo clippy`, and the existing tests pass.

## 中文

### 当前代码形态

在 `crates/liyi/src/check.rs` 中已经存在可利用的内部切分点：

- `run_check()` 负责仓库发现、构建需求状态、检查每个 sidecar、发出后置诊断、
  排序诊断并计算退出码。
- 第一遍扫描已经被拆成私有辅助函数：
  - `discover_requirements()`
  - `compute_requirement_hashes()`
  - `collect_source_related_refs()`
  - `enrich_requirements_from_sidecars()`
  - `detect_requirement_cycles()`
- 第二遍检查目前只通过私有的 `check_sidecar()` 辅助函数暴露。
- 注册表状态仍然是私有的 `HashMap<String, RequirementRecord>`，再加上几个相关集合：
  `requirements_with_sidecar`、`requirements_referenced`、`source_related_refs`，以及需求依赖图。
- `RequirementRecord` 仍是私有类型，只保存 `check.rs` 当前需要的数据：源文件、源行号、
  sidecar 中记录的哈希，以及从当前源文本重新计算得到的哈希。

关键结论是：这不是一次大型重构，而是在已经拆分好的辅助函数外包一层公共 API。

### 建议的公共 API 形态

先在 `liyi::check` 中加入公共类型，并保持 CLI 行为不变：

```rust
pub struct RequirementRegistry {
    requirements: HashMap<String, RequirementRecord>,
    requirements_with_sidecar: HashSet<String>,
    requirements_referenced: HashSet<String>,
    source_related_refs: HashSet<String>,
    requirement_cycles: Vec<Vec<String>>,
}

pub struct RequirementRecord {
    pub file: PathBuf,
    pub line: usize,
    pub hash: Option<String>,
    pub computed_hash: Option<String>,
}
```

然后暴露两个函数：

```rust
pub fn build_requirement_registry(
    root: &Path,
    all_files: &[PathBuf],
    sidecars: &[SidecarEntry],
    source_cache: &mut HashMap<PathBuf, String>,
) -> (RequirementRegistry, Vec<Diagnostic>);

pub fn check_sidecars(
    root: &Path,
    sidecars: &[SidecarEntry],
    registry: &RequirementRegistry,
    source_cache: &mut HashMap<PathBuf, String>,
    fix: bool,
    dry_run: bool,
) -> Vec<Diagnostic>;
```

`run_check()` 继续作为 CLI 便利封装存在。它应该调用 `discover()`，把发现阶段的警告转换为诊断，
调用 `build_requirement_registry()`，再调用 `check_sidecars()`，追加来自注册表的后置诊断，最后排序并计算既有退出码。

### 分步实现计划

1. **明确数据形态。**
   - 在原位置把私有 `RequirementRecord` 改成公共结构体。
   - 添加 `RequirementRegistry`，用于包裹现有第一遍扫描产生的数据。
   - 字段先保持私有；只有测试或 LSP 调用点确实需要时，再优先添加访问器。

2. **抽出第一遍扫描。**
   - 把 `run_check()` 中现有的第一遍调用移动到 `build_requirement_registry()`。
   - 通过该函数返回的诊断向量传出重复需求等发现阶段诊断。
   - 把环检测结果存入 `RequirementRegistry`；如果后置诊断需要完整上下文，则暂时不要在此处直接发出环诊断。

3. **抽出第二遍检查。**
   - 为了避免公共/私有名称冲突，把现有私有 `check_sidecar()` 改名，例如改为 `check_one_sidecar()`。
   - 新增公共 `check_sidecars()`，循环处理 sidecar，并把 `registry.requirements` 传给单个 sidecar 检查函数。
   - 完全保留当前写回语义：`fix && !dry_run` 才写 sidecar，`dry_run` 绝不写文件。

4. **把注册表后置诊断留在辅助函数或方法后面。**
   - 保持 `emit_untracked_requirements()`、`emit_unreferenced_requirements()`、
     `emit_cycle_diagnostics()` 的行为不变。
   - 可以继续由 `run_check()` 传入注册表字段调用这些函数，也可以加入内部辅助函数，例如
     `emit_registry_diagnostics()`。
   - 不要让 LSP API 承担 CLI 退出码计算职责。

5. **在添加 LSP 代码前先用测试锁住行为。**
   - 现有 golden 测试不应该出现输出变化。
   - 如有必要，添加一个聚焦的库测试，证明在包含相关需求的 fixture 上，
     `build_requirement_registry()` 加 `check_sidecars()` 与 `run_check()` 得到相同诊断。

### API 设计注意点

- `build_requirement_registry()` 应接受已经发现出来的 `all_files` 和 `sidecars`，而不是自己执行发现。
  LSP 会维护自己的发现状态，并需要从缓存的文件集合重建注册表。
- 暂时不要为公共注册表暴露可变操作。第一版 LSP 可以保守地重建；等有真实调用点后，再加入增量更新。
- `source_cache` 应继续由调用者持有。这与 LSP 设计一致，因为工作区状态会拥有缓存的源文本。
- 在本次重构中，不要移动 `crates/liyi/src/approve.rs` 中审批流程专用的需求注册表代码。
  它需要展示旧需求文本，需求不同，可以以后再统一。

### 风险

- **意外行为漂移：** 让 `run_check()` 继续作为测试基准，避免改变诊断、排序或退出码逻辑。
- **过度暴露内部：** 从最小注册表包装开始，只在 LSP 需要时添加只读访问器。
- **修复模式副作用：** 保留当前 `--fix` 操作顺序，让 sidecar 写回和 `_hints` 清理保持不变。
- **名称冲突：** 新增公共 `check_sidecars()` 包装函数，不要直接暴露当前会修改单个 sidecar 的私有辅助函数。

### 重构提交的完成标准

- `RequirementRegistry` 和 `RequirementRecord` 从 `liyi::check` 公开。
- `build_requirement_registry()` 公开，并可被 LSP 工作区复用。
- `check_sidecars()` 公开，并接受预先构建好的注册表。
- `run_check()` 对 CLI 保持源码兼容和行为兼容。
- `cargo fmt --check`、`cargo clippy` 和现有测试全部通过。
