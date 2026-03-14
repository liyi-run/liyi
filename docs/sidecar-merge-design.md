<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Sidecar Auto-Merge: Design

**Status:** Design
**Target:** Post-MVP
**Design authority:** `docs/liyi-design.md` — *fix-never-modifies-human-fields*, *fix-semantic-drift-protection*

---

## Motivation

<!-- @liyi:related fix-never-modifies-human-fields -->

`.liyi.jsonc` sidecars evolve alongside source code. When a commit modifies
both a source file and its sidecar, `git revert`, `git merge`, and `git rebase`
can produce conflict markers inside sidecar files — not because of genuine
semantic disagreement, but because tool-managed fields (`source_span`,
`source_hash`, `source_anchor`) shifted independently on each branch.

This is the same class of problem that package-manager lockfiles suffer from.
Yarn, PNPM, and Cargo each solve it by treating the lockfile as a derived
artifact: accept either side of the conflict, then re-derive from the source
of truth. liyi sidecars have the same property — most of their content is
derivable — but the current tooling does not exploit it.

The result is that sidecar files are a disproportionate source of merge pain,
especially for teams that rebase frequently or maintain long-lived branches.
Users should not need to hand-edit JSON to resolve a span conflict that the
tool can recompute in milliseconds.

---

## Key insight: derivability tiers

Not all sidecar fields have the same provenance. The merge strategy follows
from classifying each field by its source of truth:

| Field | Source of truth | Merge strategy |
|---|---|---|
| `source_span` | Source file + `tree_path` | Re-derive |
| `source_hash` | Source file bytes in span | Re-derive |
| `source_anchor` | First line of span | Re-derive |
| `tree_path` | Tree-sitter parse of source | Re-derive (tool overwrites) |
| `related` | `@liyi:related` markers in source + requirement registry | Re-derive |
| `_hints` | `liyi init` scaffold | Strip (ephemeral) |
| `intent` | Agent or human | Merge heuristic / flag |
| `reviewed` | Human gate | Merge heuristic / flag |
| `confidence` | Agent | Take surviving side |
| `item` / `requirement` | Identity key | Match key |
| `version`, `source` | File-level metadata | Take post-merge value |

The first six rows are fully derivable — conflicts in these fields are always
auto-resolvable by re-derivation. Only `intent` and `reviewed` carry
human-authored or human-gated semantics that require care.

---

## Design constraints

The following constraints are normative for the implementation.

<!-- @liyi:requirement merge-never-invents-intent -->
**Merge never invents intent.** The auto-merge phase must not synthesize,
interpolate, or modify intent text. It may select one side's intent when the
other side is unchanged, or flag a true conflict for human resolution. But it
must never produce an intent string that did not appear verbatim in either the
base, ours, or theirs version. This preserves the invariant that all intent
text traces to either a human or an identifiable agent invocation.
<!-- @liyi:end-requirement merge-never-invents-intent -->

<!-- @liyi:requirement merge-preserves-review-gate -->
**Merge preserves the review gate.** If either side of a merge sets
`"reviewed": false` (or removes the field), the merged result must not be
`"reviewed": true`. The merge must be conservative: `true` survives only if
both sides agree on `true` AND the intent text is identical on both sides.
Any doubt defaults to `false`.
<!-- @liyi:end-requirement merge-preserves-review-gate -->

<!-- @liyi:requirement merge-derived-fields-are-discarded -->
**Derived fields are discarded, not merged.** `source_span`, `source_hash`,
`source_anchor`, `tree_path`, `related`, and `_hints` from both sides of a
conflict are discarded entirely. They are recomputed by the normal `--fix`
pass that follows the merge phase. This eliminates the largest class of sidecar
conflicts (span shifts, hash drift) and avoids propagating stale derivations.
<!-- @liyi:end-requirement merge-derived-fields-are-discarded -->

<!-- @liyi:requirement merge-is-pre-phase -->
**Merge resolution is a pre-phase of `--fix`.** The conflict-resolution logic
runs before the existing check + fix passes, not as a separate subcommand.
When `--fix` encounters a sidecar with conflict markers, it resolves them
first, then proceeds with the normal hash / span / related-edge recomputation.
This means a single `liyi check --fix` after any git operation is sufficient.
<!-- @liyi:end-requirement merge-is-pre-phase -->

<!-- @liyi:requirement merge-refuses-cross-version -->
**Merge refuses to merge across schema versions.** If the `version` field
differs between ours and theirs (e.g., `"0.1"` vs `"0.2"`), the merge phase
emits a diagnostic and leaves the conflict markers intact. Schema migration
is a deliberate operation, not something to resolve silently during merge.
<!-- @liyi:end-requirement merge-refuses-cross-version -->

<!-- @liyi:requirement merge-respects-dry-run -->
**Merge pre-phase respects `--dry-run`.** When `--dry-run` is active, the
merge phase reports what it would resolve ("would auto-resolve N specs, M
need manual review") without writing any files. This is consistent with the
existing `--fix --dry-run` contract.
<!-- @liyi:end-requirement merge-respects-dry-run -->

---

## Conflict detection

A sidecar file is considered conflicted if it contains Git conflict markers
(`<<<<<<<`, `=======`, `>>>>>>>`). Detection is a byte-level scan — no JSON
parsing is attempted on unresolved files. The merge phase extracts three
regions:

- **Ours** — text between `<<<<<<<` and `=======`
- **Theirs** — text between `=======` and `>>>>>>>`
- **Base** — if diff3 markers are present (`|||||||`), the common ancestor;
  otherwise inferred from the two sides

If multiple conflict regions exist in one file, each is extracted independently
and the non-conflicting regions are preserved as-is. The concatenated result of
all resolved regions must parse as valid JSONC before proceeding to the fix
phase.

**Failure handling:** If either side of a conflict region produces unparseable
JSONC, or the concatenated result is invalid, the merge phase does not write
the file. It leaves the raw conflict markers intact and emits a diagnostic at
`Error` severity instructing the human to resolve manually. The merge phase
must never produce a corrupted sidecar.

---

## Merge algorithm

Resolution proceeds in two stages: structural merge, then field-level merge.

### Stage 1: Structural merge (spec-level)

Parse ours and theirs as JSONC (lenient — tolerating trailing commas and
comments). Match specs by identity key:

- **Item specs:** matched by identity cascade:
  1. `(item, tree_path)` — preferred when both sides have `tree_path`.
  2. `(item, source_anchor)` — when `tree_path` is absent or differs
     (common after refactoring on one branch).
  3. `(item)` alone — last resort when a single spec with that name exists
     on each side. If multiple specs share the same `item` name and no
     higher-priority key matches, flag as ambiguous (human review).
- **Requirement specs:** matched by `requirement` (unique per repo).

The identity cascade prevents the common failure mode where one branch
refactors a function (changing `tree_path`) while the other modifies its
intent — without the cascade, these appear as unrelated add/delete pairs
rather than a matched modification.

Three cases:

| Ours | Theirs | Resolution |
|---|---|---|
| Present | Present | Field-level merge (Stage 2) |
| Present | Absent | Keep (one side added or other side didn't touch) |
| Absent | Present | Keep (symmetric) |

When both sides delete the same spec, it stays deleted.

When one side deletes and the other modifies, flag for human review — this is
a genuine conflict (intent removed vs. intent changed).

### Stage 2: Field-level merge (per matched spec)

For each matched spec pair, resolve fields individually:

**Derived fields** (`source_span`, `source_hash`, `source_anchor`, `tree_path`,
`related`, `_hints`):
Discard both sides. Set `source_span` to whichever side's value parses (as a
placeholder — `--fix` will recompute). Set all others to `null` / absent. The
subsequent `--fix` pass re-derives them from source.

**`intent`:**

| Base | Ours | Theirs | Resolution |
|---|---|---|---|
| A | A | A | A (no change) |
| A | B | A | B (ours changed) |
| A | A | B | B (theirs changed) |
| A | B | B | B (both changed identically) |
| A | B | C | **Conflict** — flag for human review |
| (no base) | B | C | **Conflict** — flag for human review |

When base is unavailable (no diff3), fall back to: if ours == theirs, take
either. Otherwise, conflict.

**Sentinel intent values** (`=doc`, `=trivial`): These carry structural
meaning beyond their text. A transition between sentinel and literal intent
(e.g., one side changes `=trivial` → explicit intent, or `=doc` → literal
string) is always treated as a conflict, even if only one side changed.
Sentinel-to-sentinel changes (e.g., `=trivial` → `=doc`) are also conflicts.
The only auto-resolvable case is when both sides agree on the same sentinel.

**`@liyi:intent` in source:** When an item has `@liyi:intent` in the
post-merge source file, the source marker is authoritative regardless of
sidecar `intent` or `reviewed` values. The merge phase sets `intent` to
`=doc` (or the marker's prose if inline) and `reviewed` to `true`, since
source-level intent is definitionally reviewed. This takes precedence over
the three-way table above.

**`reviewed`:**

- If either side is `false` (or absent) → `false`
- If both sides are `true`:
  - And intent text is identical on both sides → `true`
  - And intent text differs → `false` (conservative: changed intent is
    unreviewed)

**`confidence`:**
Take the side that changed `intent`. If neither or both changed, take the
lower value (conservative). Absent beats present (conservative — no
confidence claim is safer than an inherited one from a stale context).

### Unresolvable conflicts

When the algorithm encounters an unresolvable case (both sides changed intent,
or delete-vs-modify), it:

1. Writes the merged sidecar with all resolvable fields resolved.
2. For unresolvable specs, preserves both intents in a structured marker:

   ```jsonc
   {
     "item": "process_payment",
     "intent": null,
     "_merge_conflict": {
       "ours": "Must validate currency before processing",
       "theirs": "Must reject payments over the daily limit"
     },
     "source_span": [42, 58]
   }
   ```

   **Schema compatibility:** `_merge_conflict` must be added to the
   `itemSpec` definition in `schema/liyi.schema.json` as an optional
   transient field (same lifecycle as `_hints` — stripped once resolved):

   ```json
   "_merge_conflict": {
     "type": "object",
     "properties": {
       "ours": { "type": ["string", "null"] },
       "theirs": { "type": ["string", "null"] }
     },
     "description": "Transient merge-conflict marker. Present only when both sides changed intent and the merge could not auto-resolve. Stripped by liyi check --fix after the human edits intent."
   }
   ```

3. Emits a diagnostic at `Error` severity so `liyi check` fails until the
   human resolves it. The diagnostic message references the item name and
   both candidate intents.

4. `_merge_conflict` is orthogonal to the triage workflow
   (`.liyi/triage.json`). Triage handles stale-but-parseable specs; merge
   conflicts are unparseable specs that block all downstream processing.
   `liyi triage` should skip items with `_merge_conflict` and report them
   as "blocked on merge resolution."

This avoids leaving raw Git conflict markers in JSONC (which would make the
file unparseable) while still giving the human enough information to decide.

---

## Integration with existing `--fix` flow

The merge pre-phase slots into the existing two-pass check architecture:

```
liyi check --fix
  ┌─────────────────────────────────────────────┐
  │ Pre-phase: Conflict resolution              │
  │  1. Scan sidecars for conflict markers       │
  │  2. Parse ours/theirs (lenient JSONC)        │
  │  3. Structural merge (spec identity match)   │
  │  4. Field-level merge (derivable → discard)  │
  │  5. Write resolved sidecar                   │
  │  6. Report: "Auto-resolved N specs, M need   │
  │     manual review"                           │
  └──────────────────┬──────────────────────────┘
                     ▼
  ┌─────────────────────────────────────────────┐
  │ Pass 1: Requirement discovery (unchanged)    │
  │  - discover_requirements()                   │
  │  - compute_requirement_hashes()              │
  │  - collect_source_related_refs()             │
  │  - enrich_requirements_from_sidecars()       │
  └──────────────────┬──────────────────────────┘
                     ▼
  ┌─────────────────────────────────────────────┐
  │ Pass 2: Per-sidecar validation (unchanged)   │
  │  - check_item_hash()      → recompute spans  │
  │  - check_related_edges()  → re-derive edges  │
  │  - check_requirement_hash()                   │
  │  - strip _hints                               │
  │  - writeback                                  │
  └─────────────────────────────────────────────┘
```

After the pre-phase, the sidecar is conflict-free but has placeholder spans
and null hashes. Pass 2 treats it exactly like a sidecar that was manually
edited — all derived fields get recomputed.

**Spec ordering:** The merged spec array follows source order — specs are
sorted by `source_span[0]` (start line) ascending, matching the convention
used by `liyi init` and `--fix` writeback. Specs without a valid span
(e.g., those with `_merge_conflict`) are appended at the end.

---

## Git merge driver (optional)

For teams that want conflicts resolved at merge time rather than after, liyi
can register a custom merge driver in `.gitattributes`:

```gitattributes
*.liyi.jsonc merge=liyi
```

With the driver configured in `.git/config` (or project-level `.gitconfig`):

```ini
[merge "liyi"]
    name = liyi sidecar auto-merge
    driver = liyi merge-driver %O %A %B
```

The `merge-driver` subcommand runs the same algorithm as the `--fix`
pre-phase but operates on the three files Git provides (base, ours, theirs).
If all conflicts are resolvable, it writes the result to `%A` and exits 0
(clean merge). If any conflicts require human review, it writes the
partially-resolved result (with `_merge_conflict` markers) and exits 1
(conflict), letting Git mark the file.

This is a convenience — `liyi check --fix` after an unclean merge is always
sufficient. The driver just avoids the intermediate state where sidecars have
raw conflict markers.

---

## Scope

### In scope

- Conflict marker detection in `.liyi.jsonc` files
- Three-way structural merge of spec arrays
- Field-level merge with derivability classification
- `_merge_conflict` markers for unresolvable intent conflicts
- Diagnostic emission for unresolved conflicts
- `liyi merge-driver` subcommand for Git integration
- Summary reporting ("N auto-resolved, M need review")

### Out of scope (post-MVP)

- Cross-repository requirement merge (submodule scenarios)
- Interactive conflict resolution TUI
- IDE merge-tool integration (LSP-side merge assistance)
- Merge of `.liyiignore` files (simple text, standard merge suffices)
- Automatic `rerere`-style learning from past conflict resolutions

---

## User workflow

### After `git merge` / `git rebase` / `git revert`

```sh
# Merge produces conflicts in sidecars
git merge feature-branch

# Single command resolves sidecar conflicts and recomputes derived fields
liyi check --fix

# Review any remaining conflicts (both-sides-changed-intent)
# Fix them manually, then:
liyi check --fix   # recomputes hashes for the manually resolved specs
git add -A && git commit
```

### With merge driver configured

```sh
# Sidecars auto-resolve during merge — no conflict markers
git merge feature-branch

# Verify (should be clean)
liyi check

git commit
```

---

## Analogy to lockfile mergers

| Aspect | Yarn / PNPM | liyi |
|---|---|---|
| Derived artifact | Lockfile (from package.json) | Sidecar spans, hashes, related edges (from source) |
| Source of truth | `package.json` | Source file + `tree_path` + `@liyi:related` markers |
| Human-authored fields | (none — lockfile is fully derived) | `intent`, `reviewed` |
| Merge strategy | Accept either side, re-derive | Accept either side for derived fields; heuristic merge for human fields |
| Git integration | `yarn install` post-merge / merge driver | `liyi check --fix` post-merge / merge driver |
| Key difference | Lockfile has no human fields | Sidecars mix derived and human fields → need the two-tier strategy |

The key difference — human-authored fields in sidecars — is why liyi cannot
simply "accept either side and regenerate." The two-tier approach (discard
derived, merge human) handles this cleanly.

---

## Testing strategy

The three-way merge has high combinatorial complexity. Testing should cover:

**Golden tests** (one per row of each merge table):
- Each intent merge case (unchanged / one-side-changed / both-identical /
  both-different / no-base)
- Sentinel value transitions (`=doc` ↔ literal, `=trivial` ↔ literal)
- `reviewed` merge matrix (true/true, true/false, false/false, with
  matching and divergent intents)
- `@liyi:intent` source-marker override
- Delete-vs-modify conflicts
- Schema version mismatch → diagnostic

**Property-based tests** (following the existing `shift_proptest.rs` pattern):
- Merge of arbitrary spec arrays preserves all non-conflicting specs
- Derived fields in merge output are always null/placeholder (never
  carried from either side)
- `reviewed: true` in output implies identical intent on both sides
- Merge is deterministic (same inputs → same output)
- Roundtrip: merge output parses as valid JSONC and passes schema
  validation (except for `_merge_conflict` items, which fail at
  `intent: null`)

**Conflict-marker fuzzing:**
- Nested / malformed / partial conflict markers → graceful fallback
  (leave markers intact, emit diagnostic)
- Multiple conflict regions in a single file
- Conflict markers inside JSONC string values (must not be detected
  as real conflicts)

---

## AIGC Disclaimer

This document contains content from the following AI agents:

* Claude Opus 4.6
* Hunter Alpha

The document is primarily authored by Claude Opus 4.6 with the human designer's input.
Hunter Alpha did a round of design review and most of their suggestions were integrated.
