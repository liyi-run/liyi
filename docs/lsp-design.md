<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# `liyi-lsp`: Language Server Protocol Integration

**Status:** Design
**Target:** v0.2.x
**Design authority:** `docs/liyi-design.md` v8.12

---

## Motivation

The CLI is batch-oriented — a human or CI runs `liyi check`, reads diagnostics, acts on them. An LSP server makes the same information appear in real time: diagnostics surface in the Problems panel as the developer types, code actions offer one-click approval and reanchoring, and inlay hints render intent inline beside the code it describes. The goal is to reduce the feedback loop from "save, run linter, read output, go back to editor" to "see it live."

The library crate (`liyi`) already exposes the right primitives. Most operations — sidecar parsing, item discovery, span resolution, shift detection, approval candidate collection — are pure functions that take file contents and return structured results. The LSP server is a thin async wrapper that caches project state and maps library types to LSP protocol types.

---

## Design constraints

The following constraints are normative for the implementation.

<!-- @liyi:requirement lsp-reuses-library-crate -->
**The LSP server reuses the `liyi` library crate.** All domain logic (checking, discovery, parsing, hashing, tree-path resolution, approval) lives in `liyi`. The LSP crate imports `liyi` as a dependency and calls its public API. No domain logic is duplicated or reimplemented in the LSP crate. This ensures that CLI and LSP always agree on semantics.
<!-- @liyi:end-requirement lsp-reuses-library-crate -->

<!-- @liyi:requirement lsp-diagnostics-match-cli -->
**LSP diagnostics are semantically identical to CLI diagnostics.** Every `Diagnostic` that `liyi check` would emit for a file must also appear as an LSP `Diagnostic` for that file, with the same severity and equivalent message text. The LSP may present additional UI affordances (code actions, related information links), but must not suppress, downgrade, or invent diagnostics that the CLI would not produce. Users must be able to trust that a clean LSP session implies a clean `liyi check` run.
<!-- @liyi:end-requirement lsp-diagnostics-match-cli -->

<!-- @liyi:requirement lsp-requirement-registry-cached -->
**The requirement registry is computed once and incrementally maintained.** Pass 1 of `run_check` (requirement discovery) scans every file in the project for `@liyi:requirement` markers. The LSP must not re-scan the entire project on every keystroke. Instead, it builds the registry on workspace open and updates it incrementally when files containing requirement markers are created, modified, or deleted. The registry is the source of truth for requirement name uniqueness, cycle detection, and related-edge validation.
<!-- @liyi:end-requirement lsp-requirement-registry-cached -->

<!-- @liyi:requirement lsp-no-domain-logic-in-server -->
**No domain logic in the server crate.** The LSP crate contains only: protocol handling (request/notification dispatch), state management (caching, incremental updates), and type conversion (library types → LSP types). Any logic that could be tested without an LSP harness belongs in the library crate instead. If a refactor to the library is needed to expose the right granularity, that refactor happens in `liyi`, not by reimplementing in `liyi-lsp`.
<!-- @liyi:end-requirement lsp-no-domain-logic-in-server -->

<!-- @liyi:requirement lsp-check-refactor-exposes-registry -->
**`run_check` is refactored to expose a cacheable requirement registry.** The current `run_check` function is monolithic — Pass 1 (requirement discovery) and Pass 2 (per-sidecar validation) share a single function scope. For the LSP, Pass 1 must be extractable as a standalone function that returns a `RequirementRegistry`, and Pass 2 must accept that registry as input. This refactor happens in the `liyi` library crate and benefits both CLI and LSP: the CLI calls them sequentially as before; the LSP caches the registry across edits.
<!-- @liyi:end-requirement lsp-check-refactor-exposes-registry -->

<!-- @liyi:requirement lsp-synchronous-library-async-server -->
**The library stays synchronous; the server is async.** The `liyi` library crate has zero async dependencies (no tokio, no futures). This remains true. The LSP server wraps synchronous library calls in `spawn_blocking` (or equivalent). This keeps the library simple, testable, and usable from non-async contexts (CLI, WASM, test harnesses).
<!-- @liyi:end-requirement lsp-synchronous-library-async-server -->

<!-- @liyi:requirement lsp-sidecar-source-pairing -->
**Sidecar and source are always paired.** When the LSP processes a file event (open, change, save), it must resolve the sidecar↔source pairing: a source file event triggers re-checking its sidecar (if any), and a sidecar file event triggers re-checking its paired source file. The pairing follows the naming convention defined in the `liyi-sidecar-naming-convention` requirement. Diagnostics are published for both files in the pair when either changes.
<!-- @liyi:end-requirement lsp-sidecar-source-pairing -->

---

## Crate layout

```
crates/
  liyi/          # library crate (unchanged API, internal refactor)
  liyi-cli/      # CLI binary (unchanged)
  liyi-lsp/      # new: LSP server binary
    Cargo.toml
    src/
      main.rs
```

`liyi-lsp` depends on `liyi` (library) + `tower-lsp` + `tokio`. The CLI is unaffected.

---

## Library refactoring (prerequisite)

Before the LSP crate can be written, `run_check` needs a targeted refactor to expose its two passes as composable units.

### Current shape

```rust
pub fn run_check(
    root: &Path,
    scope_paths: &[PathBuf],
    fix: bool,
    dry_run: bool,
    flags: &CheckFlags,
) -> (Vec<Diagnostic>, LiyiExitCode)
```

Internally: Pass 1 builds a `HashMap<String, RequirementRecord>` (requirement registry), then Pass 2 iterates sidecars and validates each against the registry. Both passes share local variables in one function body.

### Target shape

```rust
/// Pass 1: scan all files for @liyi:requirement markers.
/// Returns the registry and any diagnostics from discovery itself
/// (duplicates, cycles).
pub fn build_requirement_registry(
    root: &Path,
    all_files: &[PathBuf],
    source_cache: &mut HashMap<PathBuf, String>,
) -> (RequirementRegistry, Vec<Diagnostic>)

/// Pass 2: validate sidecars against a pre-built registry.
pub fn check_sidecars(
    root: &Path,
    sidecars: &[SidecarEntry],
    registry: &RequirementRegistry,
    source_cache: &mut HashMap<PathBuf, String>,
    fix: bool,
    dry_run: bool,
    flags: &CheckFlags,
) -> Vec<Diagnostic>

/// Convenience wrapper that calls both passes (CLI entry point).
pub fn run_check(
    root: &Path,
    scope_paths: &[PathBuf],
    fix: bool,
    dry_run: bool,
    flags: &CheckFlags,
) -> (Vec<Diagnostic>, LiyiExitCode)
```

`RequirementRegistry` is a new public type wrapping the existing `HashMap<String, RequirementRecord>` plus the source-related-refs set. `RequirementRecord` becomes public. The CLI continues calling `run_check`, which calls both passes internally. The LSP calls `build_requirement_registry` once on startup and `check_sidecars` on each file change, rebuilding the registry only when requirement markers change.

This refactor is a pure extraction — the logic does not change, only its visibility and calling convention. It should be done as a separate commit before the LSP crate is added.

---

## LSP capabilities

### Phase 1 — Diagnostics and file watching

The minimum viable LSP: publish diagnostics in real time.

**Capabilities declared:**
- `textDocumentSync`: `Full` (simplest; `Incremental` is a future optimization)
- `workspace/didChangeWatchedFiles`: watch `**/*.liyi.jsonc` and all source files

**Lifecycle:**
1. `initialize`: receive workspace root, store it
2. `initialized`: run full discovery + `build_requirement_registry` + `check_sidecars` for all sidecars. Publish diagnostics for every file with issues
3. `textDocument/didOpen`, `textDocument/didChange`, `textDocument/didSave`: re-parse the changed file's sidecar (or the sidecar's source), re-run `check_sidecars` for the affected pair, republish diagnostics
4. `workspace/didChangeWatchedFiles`: handle sidecar/source creation and deletion — update discovery cache, rebuild registry if requirement markers were affected

**Diagnostic mapping:**

| `DiagnosticKind` | LSP severity | Code | Notes |
|---|---|---|---|
| `Current` | — | — | Not published (clean) |
| `Trivial` | — | — | Not published (clean) |
| `Stale` | Warning | `stale` | |
| `Shifted { from, to }` | Information | `shifted` | With `--fix` available via code action |
| `Unreviewed` | Information | `unreviewed` | Depends on `--fail-on-unreviewed` |
| `ReqChanged` | Warning | `req-changed` | |
| `Untracked` | Warning | `untracked` | |
| `ParseError` | Error | `parse-error` | |
| `OrphanedSource` | Warning | `orphaned` | |
| `InvalidSpan` | Error | `invalid-span` | |
| `MalformedHash` | Error | `malformed-hash` | |
| `SpanPastEof` | Error | `span-past-eof` | |
| `DuplicateEntry` | Error | `duplicate` | |
| `RequirementCycle` | Error | `cycle` | |
| `AmbiguousSidecar` | Warning | `ambiguous-sidecar` | |
| `MissingRelatedEdge` | Warning | `missing-related` | |
| `ReqNoRelated` | Information | `req-no-related` | |
| `ConflictingTriviality` | Warning | `conflicting-triviality` | |

Diagnostics are placed at `span_start` (converted to 0-indexed) when available, or at line 0 for file-level issues. Range end is the same line (line-level granularity; column-level precision is a future enhancement when AST node ranges are propagated).

### Phase 2 — Code actions

**Capabilities declared:**
- `textDocument/codeAction`

| Action | Trigger | Implementation |
|---|---|---|
| **Reanchor spans** | `Shifted` diagnostic | Call `run_reanchor()` on the sidecar, apply as workspace edit |
| **Approve item** | `Unreviewed` diagnostic | Call `apply_approval_decisions()` with `Decision::Yes` |
| **Reject item** | `Unreviewed` diagnostic | Call `apply_approval_decisions()` with `Decision::No` |
| **Scaffold sidecar** | Source file with no sidecar | Call `init_sidecar()`, open the new file |
| **Fix all** | Any `--fix`-eligible diagnostic | Call `check_sidecars` with `fix=true` for this sidecar |

Code actions return `WorkspaceEdit` objects that modify the sidecar file. The server re-reads the sidecar after the edit is applied and republishes diagnostics.

### Phase 3 — Inlay hints and CodeLens

**Capabilities declared:**
- `textDocument/inlayHint`
- `textDocument/codeLens`

**Inlay hints:** For each item spec in the sidecar, resolve the source span (via `resolve_tree_path` or line number). Display the intent text as a trailing hint after the item's signature line. Distinguish reviewed (solid) from unreviewed (faded/italic) via hint kind.

**CodeLens:** Above each item with a spec, show a lens with the review status: "✓ reviewed" or "⚠ unreviewed — click to approve." Clicking triggers the approve code action.

### Phase 4 — Completion and hover (future)

Out of scope for initial implementation. Potential future features:
- Completion inside `.liyi.jsonc` files (requirement names in `related` fields, item names)
- Hover over `@liyi:related <name>` annotations in source showing the requirement text

---

## State management

The LSP server maintains the following cached state:

```
WorkspaceState {
    root: PathBuf,
    requirement_registry: RequirementRegistry,
    sidecars: Vec<SidecarEntry>,
    source_cache: HashMap<PathBuf, String>,
    diagnostics: HashMap<PathBuf, Vec<Diagnostic>>,
}
```

**Invalidation rules:**

| Event | Action |
|---|---|
| Source file changed | Clear source cache for that file; re-check its sidecar pair |
| Sidecar file changed | Re-parse the sidecar; re-check the sidecar pair |
| Source file created/deleted | Re-run discovery; rebuild registry if markers present |
| Sidecar file created/deleted | Re-run discovery; re-check the affected source |
| `.liyiignore` changed | Full re-discovery |
| Requirement marker added/removed | Rebuild requirement registry; re-check all sidecars with `related` edges |

The most expensive operation — rebuilding the requirement registry — is triggered only when `@liyi:requirement` markers are added, removed, or renamed. For typical editing (changing code within existing functions), only the affected sidecar pair is re-checked.

---

## Scope

### In scope

- LSP server binary (`liyi-lsp`) as a new crate in `crates/`
- Library refactoring: extract `build_requirement_registry` and `check_sidecars` from `run_check`
- Phase 1 (diagnostics) and Phase 2 (code actions) as described above
- VS Code client extension skeleton (separate repository, references `liyi-lsp` binary)

### Out of scope

- Phase 3 (inlay hints, CodeLens) and Phase 4 (completion, hover) — deferred to follow-up
- WASM compilation of the library crate
- Incremental text sync (`TextDocumentSyncKind::Incremental`) — start with `Full`
- Multi-root workspace support — start with single root
- Embedded language injection discovery in the LSP (tree-sitter injection works, but real-time re-parsing of injected regions on every keystroke is deferred)

---

## Dependencies

The `liyi-lsp` crate adds:

| Crate | Purpose |
|---|---|
| `tower-lsp` | LSP protocol framework (async, tower-based) |
| `tokio` | Async runtime (required by tower-lsp) |
| `liyi` | Domain logic (workspace dependency) |

No new dependencies are added to the `liyi` library crate. The CLI crate (`liyi-cli`) is unaffected.

---

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Binary size (~30–50 MB) due to 23 tree-sitter grammars | Acceptable for a local development tool. Feature-gate grammars if users request lighter builds |
| Incorrect incremental invalidation → stale diagnostics | Conservative strategy: re-check sidecar pair on any change. Full re-check on doubt. Correctness over performance |
| `run_check` refactor breaks CLI | Refactor preserves `run_check` as a convenience wrapper. CLI behavior is unchanged. Test suite catches regressions |
| `spawn_blocking` contention under rapid typing | Debounce file change events (200ms). Cancel in-flight checks when a newer version arrives |

---

## Roadmap

### Step 0: Library refactoring

Extract `build_requirement_registry` and `check_sidecars` from `run_check`. Make `RequirementRegistry` and `RequirementRecord` public types.

**Acceptance criteria:**
- `run_check` calls `build_requirement_registry` then `check_sidecars` internally
- All existing tests pass with no behavioral change
- `cargo clippy` and `cargo fmt --check` clean

### Step 1: Scaffold `liyi-lsp` crate

Create `crates/liyi-lsp/` with `Cargo.toml`, `main.rs`. Implement `initialize`/`initialized`/`shutdown`. Verify the binary starts and responds to LSP handshake.

### Step 2: Phase 1 — Diagnostics

Implement file watching, sidecar pair resolution, incremental re-checking, diagnostic publishing. This is the core value — everything after is incremental.

**Acceptance criteria:**
- Opening a workspace publishes diagnostics for all files with issues
- Editing a source file updates diagnostics in real time
- Creating/deleting a sidecar updates diagnostics
- Diagnostics match `liyi check` output for the same files

### Step 3: Phase 2 — Code actions

Implement reanchor, approve, reject, scaffold, fix-all actions.

**Acceptance criteria:**
- Each action produces a correct workspace edit
- Diagnostics update after action is applied

### Step 4: VS Code extension

Separate repository. Minimal extension that launches `liyi-lsp` and contributes status bar, settings (binary path, enable/disable), and a "Scaffold Sidecar" command.
