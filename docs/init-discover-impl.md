<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# `liyi init` Item Discovery & Hints: Implementation Plan

**Status:** Implementation plan, not yet started
**Target:** v0.1.x (M10)
**Design authority:** `docs/liyi-design.md` v8.9 — *Tree-sitter item discovery in scaffold*, *`_hints` — cold-start inference aids*

---

## Motivation

Today `liyi init <source-file>` emits a skeleton sidecar with `"specs": []`. The agent must discover every function, struct, and class by reading the source itself. This wastes token budget on mechanical work that tree-sitter can do deterministically and instantaneously.

Smart scaffold pre-populates the `specs` array with every item discovered in the AST, complete with `source_span`, `tree_path`, and optional `_hints`. The agent's job shifts from *discovering structure* to *inferring intent* — a better use of its reasoning capability.

---

## Design constraints

The following constraints are normative for the implementation.

<!-- @liyi:requirement exhaustive-inclusion -->
**Exhaustive inclusion.** Every item discovered by tree-sitter gets a spec entry. Over-generation with agent pruning is strictly better than under-generation with agent creation — a missing item is a silent coverage gap; an extra `"=trivial"` entry is an audit trail. The agent removes or annotates; the scaffold never omits.
<!-- @liyi:end-requirement exhaustive-inclusion -->

<!-- @liyi:requirement hints-are-ephemeral -->
**Hints are ephemeral.** `_hints` exist only during the cold-start inference window. `liyi init` writes them; the agent reads them; `liyi check --fix` strips them. Hints must never appear in steady-state committed sidecars. The linter ignores their presence (no error), but `--fix` removes them unconditionally.
<!-- @liyi:end-requirement hints-are-ephemeral -->

<!-- @liyi:requirement hints-intentionally-unstructured -->
**Hints are intentionally unstructured.** The `_hints` field is `"type": "object"` with no further property constraints in the JSON Schema. The consumer is an LLM, not a parser. No downstream tooling may depend on a specific `_hints` shape. This prevents accidental coupling to an ephemeral inference aid and allows `liyi init` to evolve what it emits without breaking anything.
<!-- @liyi:end-requirement hints-intentionally-unstructured -->

<!-- @liyi:requirement tree-sitter-signals-always-present -->
**Tree-sitter signals are always present.** When a language has a grammar, tree-sitter-derived hints (`_body_lines`, `_has_doc`, `_likely_trivial`) are always emitted — they require no flags and no VCS. VCS signals (`commits`, `fix_commits`, `has_tests`, `last_modified_days`) require `--hints` and a git repository. Without `--hints`, the scaffold still discovers items and populates `source_span` + `tree_path`; only VCS signals are omitted.
<!-- @liyi:end-requirement tree-sitter-signals-always-present -->

<!-- @liyi:requirement graceful-degradation -->
**Graceful degradation.** When a language has no tree-sitter grammar, `liyi init` falls back to the current empty `"specs": []` skeleton. When `--hints` is requested outside a git repository, VCS signals are silently omitted. The tool never fails because of missing optional context.
<!-- @liyi:end-requirement graceful-degradation -->

<!-- @liyi:requirement no-git2-dependency -->
**Shell out to git, do not add `git2`.** The existing `git.rs` module shells out to the `git` CLI. VCS signal gathering must follow the same pattern. The `git2` crate links libgit2 (a C library), which adds ~2 MB to the binary size, complicates cross-compilation, and introduces a second code path for git operations. One git integration strategy means one debugging surface.
<!-- @liyi:end-requirement no-git2-dependency -->

<!-- @liyi:requirement item-naming-uses-leaf-name -->
**Item naming uses leaf name for non-nested items, qualified name for nested.** Top-level items use their plain name (`"item": "add_money"`). Items nested inside a container use the container-qualified form (`"item": "Money::new"`). This matches the existing convention in hand-written sidecars throughout this repository.
<!-- @liyi:end-requirement item-naming-uses-leaf-name -->

<!-- @liyi:requirement reuse-kind-map-as-item-definition -->
**`kind_map` is the sole definition of "item".** An AST node is an item if and only if `LanguageConfig::kind_to_shorthand()` returns `Some`. No parallel list, no ad-hoc node-kind checks. This ensures that adding a new item kind to a language is a single-point change.
<!-- @liyi:end-requirement reuse-kind-map-as-item-definition -->

---

## Scope

### In scope

- `discover_items()` function: full-file AST traversal returning all items with span, tree_path, and name.
- Tree-sitter hints: `_body_lines`, `_has_doc`, `_likely_trivial`.
- VCS hints (behind `--hints`): `commits`, `fix_commits`, `has_tests`, `last_modified_days`.
- Doc comment detection for the 6 highest-traffic languages (Rust, Python, Go, JavaScript, TypeScript, Java).
- `_hints` field on `ItemSpec` (as `serde_json::Value`).
- `_hints` stripping in `liyi check --fix`.
- `--no-discover` flag to opt out of item discovery.
- `--hints` flag to opt in to VCS signals.
- `--trivial-threshold <N>` flag (default: 5).
- Golden test fixture for scaffold output.

### Out of scope

- Doc comment detection for remaining languages (C, C++, C#, PHP, Objective-C, Kotlin, Swift, Bash, Ruby, Zig) — deferred; `_has_doc` is simply absent for these until implemented.
- Content extraction from doc comments (the tool detects *presence*, not *text*).
- `liyi init <directory>` batch mode — deferred; the current CLI accepts one file.
- `_hints` in `liyi check --prompt` output — deferred to M5.3.

---

## Architecture

### Item discovery: `discover_items()`

New public function in `tree_path/mod.rs`:

```rust
pub struct DiscoveredItem {
    /// Display name (leaf or container-qualified).
    pub name: String,
    /// 1-indexed inclusive span [start, end].
    pub span: [usize; 2],
    /// Canonical tree_path (e.g., "impl.Money::fn.new").
    pub tree_path: String,
}

pub fn discover_items(source: &str, lang: Language) -> Vec<DiscoveredItem>
```

Implementation: depth-first traversal of the parsed AST. At each node, check `is_item_node()` (reuses existing `kind_map`). For item nodes, extract the name via `node_name()` (reuses existing custom name logic), compute the `tree_path` via `build_path_to_node()`, and record the span.

This composes existing helpers with zero new tree-sitter queries or grammar changes. The `kind_map` already exhaustively defines which node types are items for all 17 supported languages.

Nested items (methods inside `impl`, `class`, etc.) are discovered naturally by the recursive walk. The `build_path_to_node` function already constructs the full qualified path from root to target, handling all container nesting.

### Doc comment detection

New method on `LanguageConfig`:

```rust
/// Optional callback to detect whether a doc comment is attached to an item node.
/// Returns true if a doc comment immediately precedes or is inside the item.
doc_comment_detector: Option<fn(&Node, &str) -> bool>,
```

Language-specific detection:

| Language | Doc comment form | Detection strategy |
|---|---|---|
| Rust | `///`, `//!`, `/** */` | Previous sibling is `line_comment` starting with `///` or `block_comment` starting with `/**` |
| Python | `"""..."""` docstring | First child of function/class body is `expression_statement` containing `string` |
| Go | `// Comment` | Previous sibling is `comment` node |
| JavaScript/TypeScript | `/** ... */` | Previous sibling is `comment` starting with `/**` |
| Java | `/** ... */` | Previous sibling is `block_comment` starting with `/**` |

For languages without a detector, `_has_doc` is simply not emitted (graceful degradation per constraint above).

### VCS signal gathering

New functions in `git.rs`, following the existing shell-out pattern:

```rust
/// Count commits and fix-commits that touched lines [start, end] in a file.
pub fn git_log_line_range(
    repo_root: &Path,
    repo_relative_path: &str,
    start_line: usize,
    end_line: usize,
) -> Option<VcsSignals>

pub struct VcsSignals {
    pub commits: usize,
    pub fix_commits: usize,
    pub last_modified_days: u64,
}
```

Uses `git log -L <start>,<end>:<file> --format=%H%n%s` to get per-line-range commit history. Fix commits are identified by a case-insensitive regex on the commit subject: `\bfix(es|ed)?\b|\bbug\b|\bpatch\b`.

Test presence (`has_tests`) uses a filename heuristic: for `src/foo.rs`, check existence of `tests/foo.rs`, `src/foo_test.rs`, `test_foo.rs`, etc. This is language-convention-dependent and best-effort.

### `_hints` on `ItemSpec`

The `ItemSpec` struct currently uses `#[serde(deny_unknown_fields)]`. To accept `_hints` without a rigid schema:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub _hints: Option<serde_json::Value>,
```

The `deny_unknown_fields` attribute must be relaxed to `deny_unknown_fields` with a `_hints` field, or switched to a `#[serde(flatten)]` approach. Since `_hints` is the only "extra" field and is well-defined in the JSON Schema, adding it as an explicit `Option<Value>` is cleaner than flattening.

### `_hints` stripping in `check --fix`

In the `--fix` code path (currently in `check.rs` / `reanchor.rs`), after all span/hash updates, iterate specs and set `_hints` to `None` before writing the sidecar. This is unconditional — `--fix` always strips hints.

### Scaffold generation flow

```
liyi init <source-file> [--hints] [--no-discover] [--trivial-threshold N]
```

1. Detect language via `detect_language(path)`.
2. If language is `None` or `--no-discover`: emit empty skeleton (current behavior).
3. Read source file contents.
4. Call `discover_items(source, lang)` → `Vec<DiscoveredItem>`.
5. For each discovered item:
   a. Compute tree-sitter hints (`_body_lines`, `_has_doc` if detector exists, `_likely_trivial`).
   b. If `--hints` and in a git repo: gather VCS signals per item span.
   c. Build `ItemSpec` with `intent: ""`, populate `_hints`.
6. Serialize as JSONC with header comment.
7. Write sidecar.

---

## Roadmap

### Phase 1: Core item discovery

Implement `discover_items()` and wire it into `liyi init`.

**Deliverables:**
- `discover_items()` in `tree_path/mod.rs`.
- `DiscoveredItem` struct.
- Updated `init_sidecar()` to call discovery and populate specs.
- `--no-discover` CLI flag.
- `_hints` field added to `ItemSpec` in `sidecar.rs`.
- Basic golden test: multi-item Rust file → pre-populated sidecar.

**Acceptance criteria:**
- `liyi init foo.rs` produces a sidecar with one entry per item.
- Items have `item`, `source_span`, `tree_path` populated; `intent` is `""`.
- Nested items (e.g., methods inside `impl`) produce correct qualified names and tree_paths.
- Works for all 17 supported languages.
- `liyi init --no-discover foo.rs` produces empty `"specs": []`.

### Phase 2: Tree-sitter hints

Add `_body_lines`, `_has_doc`, and `_likely_trivial` to scaffold output.

**Deliverables:**
- `doc_comment_detector` callback on `LanguageConfig`.
- Detectors for Rust, Python, Go, JavaScript, TypeScript, Java.
- `--trivial-threshold` CLI flag.
- `_hints` stripping in `liyi check --fix`.
- Golden test verifying hints content.

**Acceptance criteria:**
- Scaffold entries include `_hints._body_lines` equal to `span[1] - span[0] + 1`.
- Items with doc comments get `_hints._has_doc: true` (for languages with detectors).
- Items with `_body_lines ≤ threshold` and no doc comment get `_hints._likely_trivial: true`.
- `liyi check --fix` strips all `_hints` fields from sidecars.

### Phase 3: VCS hints

Add commit-count and related signals behind `--hints`.

**Deliverables:**
- `git_log_line_range()` in `git.rs`.
- `VcsSignals` struct.
- Test-presence heuristic.
- `--hints` CLI flag.
- Golden test with mock git history (or integration test).

**Acceptance criteria:**
- `liyi init --hints foo.rs` populates `_hints.commits`, `_hints.fix_commits`, `_hints.last_modified_days`.
- `_hints.has_tests` is present when heuristic finds a test file.
- Outside a git repo, `--hints` silently omits VCS signals but retains tree-sitter signals.
- Performance: VCS gathering for a 50-item file completes in < 5 seconds on a typical repo.

### Phase 4: `=trivial` sentinel

Support `"intent": "=trivial"` in `liyi check` and `liyi approve`.

**Deliverables:**
- `liyi check` treats `=trivial` the same as `@liyi:trivial` source annotation.
- `liyi approve` can transition `=trivial` items to `"reviewed": true`.
- `ConflictingTriviality` diagnostic when `@liyi:nontrivial` in source conflicts with `=trivial` in sidecar.
- Schema update documenting `=trivial`.

**Acceptance criteria:**
- `=trivial` items are excluded from coverage reports and test generation.
- `--fail-on-untracked` does not flag `=trivial` items.
- `@liyi:nontrivial` + `=trivial` emits `ConflictingTriviality`.

---

## Open questions

1. **Should containers (e.g., `impl.Money` with no methods of its own) be emitted?** The design doc example shows them emitted but the agent removing them. Emitting is consistent with exhaustive inclusion, but creates noise. Current plan: emit them, let agents prune.

2. **`git log -L` performance.** Per-item line-range log queries can be slow on large repos. Possible mitigation: batch all items into a single `git log` call per file and distribute results, or use `git log --format` on the whole file and post-filter by line ranges via `git blame`.

3. **Threshold for `_likely_trivial`.** The design doc suggests 3–5 lines. The `--trivial-threshold` flag makes this configurable; the default should match the most common convention. Current plan: default to 5.

4. **Doc comment detection for macro-generated items.** Items produced by macro expansion may not have doc comments in the tree-sitter AST. This is acceptable — `_has_doc: false` is correct (the tool sees what tree-sitter sees).
