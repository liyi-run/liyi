<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Refactoring Roadmap

This document catalogues concrete, low-risk refactoring opportunities in the
linter crate (`crates/liyi`).  Each item is scoped to a single logical change
that can be landed as an independent commit.

Items are grouped into phases.  Within a phase the order is a suggestion, not
a constraint — any item can be tackled independently.

---

## Phase 1 — Extract shared primitives

Small, self-contained extractions that later phases build on.

### 1.1  Shared hash-validation regex

`Regex::new(r"^sha256:[0-9a-f]+$").unwrap()` is compiled at runtime in both
`sidecar.rs` and `check.rs`.  Extract a single `LazyLock<Regex>` (or a plain
`fn is_valid_hash(s: &str) -> bool`) into `hashing.rs` and use it everywhere.

**Files**: `hashing.rs`, `sidecar.rs`, `check.rs`

### 1.2  `walk_git_history` helper

The `git_log_revisions` / `git_show` loop pattern appears five times in
`approve.rs`.  Extract a generic higher-order function:

```rust
fn walk_git_history<T>(
    root: &Path,
    rel: &str,
    max: usize,
    f: impl FnMut(&str) -> Option<T>,
) -> Option<T>
```

Each existing call site collapses to a one-liner.

**Files**: `git.rs` (new helper), `approve.rs` (call-site simplification)

---

## Phase 2 — Parameter bundles in `check.rs`

Four functions in `check.rs` suppress `clippy::too_many_arguments`, each
taking 8–10 parameters.  This phase bundles the common ones into a context
struct and propagates it.

### 2.1  Introduce `ItemCheckCtx`

Define a struct that carries the shared read-only context threaded through
`check_item_hash`, `handle_hash_mismatch`, `handle_past_eof`, and
`handle_tree_path_resolved`:

```rust
struct ItemCheckCtx<'a> {
    file: &'a Path,
    source_content: &'a str,
    source_markers: &'a [SourceMarker],
    fix: bool,
}
```

### 2.2  Migrate call sites to `ItemCheckCtx`

Replace the expanded parameter lists in the four functions with
`&ItemCheckCtx`, removing all `#[allow(clippy::too_many_arguments)]`.

**Files**: `check.rs`

---

## Phase 3 — Unified span-recovery logic

The tree-path → sibling-scan → shift-heuristic cascade is duplicated between
`check.rs` and `reanchor.rs`.

### 3.1  Extract `recover_item_span`

Create a shared function (e.g. in a new `recovery.rs` or on `ItemSpec`)
that encapsulates the cascade:

1. Attempt `resolve_tree_path`.
2. On hash mismatch, try `resolve_tree_path_sibling_scan`.
3. Fall back to shift heuristic.

Returns an enum describing the outcome (unchanged / shifted / failed).

**Files**: new `recovery.rs`, `check.rs`, `reanchor.rs`

### 3.2  Wire `check.rs` and `reanchor.rs` through the shared helper

Replace the inline recovery logic in both modules with calls to
`recover_item_span`.  This eliminates ~100 lines of duplication and
guarantees the two code paths stay in sync.

**Files**: `check.rs`, `reanchor.rs`

---

## Phase 4 — Diagnostic construction helpers

`Diagnostic { … }` struct literals with 10 fields are repeated ~30 times in
`check.rs`.

### 4.1  Add `Diagnostic` constructor methods

Introduce a small set of named constructors on `Diagnostic`
(e.g. `Diagnostic::current(…)`, `Diagnostic::stale(…)`,
`Diagnostic::shifted(…)`) that fill in the boilerplate fields.

**Files**: `diagnostics.rs`, `check.rs`

---

## Phase 5 — `approve.rs` collection split

`collect_approval_candidates` is ~195 lines with three independent
collection blocks (unreviewed / stale-reviewed / requirement-changed).

### 5.1  Split into focused collectors

Extract `collect_unreviewed`, `collect_stale`, `collect_req_changed` as
standalone functions.  `collect_approval_candidates` becomes a thin
orchestrator that merges results.

**Files**: `approve.rs`

---

## Phase 6 — Language-config macro for `tree_path/`

Each `lang_*.rs` file defines a near-identical `LanguageConfig` static.

### 6.1  Declare `declare_language!` macro

Write a declarative macro that generates a `LanguageConfig` static from a
compact DSL, enforcing structural consistency and cutting per-language
boilerplate from ~50 lines to ~10.

### 6.2  Migrate existing language configs

Convert each `lang_*.rs` to use the macro.

**Files**: `tree_path/mod.rs`, all `tree_path/lang_*.rs`

---

## Non-goals

- **No behavioral changes.**  Every item above is a pure refactor — tests
  must pass identically before and after.
- **No new dependencies** except possibly `LazyLock` (stabilised since
  Rust 1.80).
- **No speculative abstractions.**  Each extraction is motivated by existing
  duplication, not hypothetical future needs.
