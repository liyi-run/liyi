<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Throwaway Roadmap: LSP Library Refactor

**Status:** temporary planning note, intended to be deleted or folded into
`docs/next-steps.md` / `docs/lsp-design.md` after the refactor lands.

**Target backlog items:** `docs/next-steps.md` 2.3 and 2.4 — expose a
cacheable requirement registry API before starting the LSP crate.

## Current code shape

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

### Duplication to be aware of (out of scope, but motivating)

`crates/liyi/src/approve.rs` independently reimplements the same pass-1
pipeline. Its private `build_requirement_registry()` returns
`HashMap<String, RequirementInfo>` and its own doc comment states it
"mirrors the pass-1 logic in `check.rs` but is self-contained." The two
registries carry **different field sets** for different needs:

- `check.rs` `RequirementRecord`: `{ file, line, hash, computed_hash }`
  (staleness comparison via stored-vs-computed hash).
- `approve.rs` `RequirementInfo`: `{ source_path, sidecar_path,
  source_span, current_hash, current_text }` (richer display + git
  history lookup for previous requirement text).

Unifying these is **explicitly deferred** (see API design notes), but the
public `RequirementRegistry` should be designed so the approve side can
eventually adopt it. In practice that means the eventual unified record is
a *superset* of both shapes, exposed through accessors rather than public
fields, so adding `source_span` / `current_text` later does not break
call sites. This is the real long-term payoff of the extraction: one
requirement registry instead of two drifting copies.

## Proposed public API shape

Add public types in `liyi::check` first, without changing CLI behavior:

```rust
pub struct RequirementRegistry {
    requirements: HashMap<String, RequirementRecord>,
    requirements_with_sidecar: HashSet<String>,
    requirements_referenced: HashSet<String>,
    source_related_refs: HashSet<String>,
    // Dependency graph retained alongside the precomputed cycles: the graph
    // is the input to detect_requirement_cycles(); cycles are its output.
    // Keep both — the graph may be needed for future incremental rechecks.
    req_dep_graph: HashMap<String, Vec<String>>,
    requirement_cycles: Vec<Vec<String>>,
}
```

Note how the post-pass emitters consume these collections in specific
combinations — do not collapse them during the move:

- `emit_untracked_requirements()` needs `requirements` +
  `requirements_with_sidecar`.
- `emit_unreferenced_requirements()` needs **both**
  `requirements_referenced` *and* `source_related_refs` (a requirement is
  "referenced" if either an item's `related` edge or a source-level
  `@liyi:related` marker points at it). Merging these two sets would
  change diagnostics.
- `emit_cycle_diagnostics()` needs `requirement_cycles` + `requirements`
  (for the file path of the first node in each cycle).

```rust
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

## Step-by-step implementation plan

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
   - Store both the dependency graph and the precomputed cycles in
     `RequirementRegistry`; the graph is the input to
     `detect_requirement_cycles()` and the cycles are its output. Do not
     emit cycle diagnostics yet if post-pass emission needs the final
     registry context.

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

## API design notes

- `build_requirement_registry()` should accept discovered `all_files` and
  `sidecars` rather than run discovery itself. The LSP will maintain its own
  discovery state and needs to rebuild the registry from cached file sets.
- The public registry should not expose mutation hooks yet. The first LSP can
  rebuild conservatively; incremental mutation can come after the API has real
  call sites.
- `source_cache` should remain caller-owned. This matches the LSP design, where
  the workspace state owns cached source text.
- Do not move approval-specific requirement registry code from
  `crates/liyi/src/approve.rs` in this refactor. Its `RequirementInfo`
  carries `source_span` and `current_text` for git-history display that
  `check.rs` does not need today, so forcing a shared shape now would
  bloat the check path. Unify later, once the public `RequirementRegistry`
  has real LSP call sites and the superset of fields is justified. Track
  this as the follow-up to next-steps 2.3/2.4.

## Risks

- **Accidental behavior drift:** keep `run_check()` as the test oracle and avoid
  changing diagnostics, sorting, or exit-code logic.
- **Over-exposing internals:** start with a minimal registry wrapper and add
  read accessors only when the LSP needs them.
- **Fix-mode side effects:** preserve the current order of `--fix` operations
  so sidecar writes and `_hints` stripping remain unchanged.
- **Name collision:** introduce a public `check_sidecars()` wrapper rather than
  exposing the current mutating one-sidecar helper directly.

## Done criteria for the refactor commit

- `RequirementRegistry` and `RequirementRecord` are public from `liyi::check`.
- `build_requirement_registry()` is public and reusable by an LSP workspace.
- `check_sidecars()` is public and accepts a prebuilt registry.
- `run_check()` remains source-compatible and behavior-compatible for the CLI.
- `cargo fmt --check`, `cargo clippy`, and the existing tests pass.
