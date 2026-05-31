<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
<!-- AIGC disclaimer: this document was drafted by an AI agent and has not been reviewed by the maintainer. -->

# Prioritized Next Steps

**As of**: 2026-05-30 · **Baseline**: v0.1.0, prompt-mode expansion and approval workflow shipped; doc-comment detection complete for all feasible languages.

This document synthesizes the existing roadmaps (liyi-design.md, lsp-design.md,
approve-impl.md, init-discover-impl.md, prompt-mode-design.md,
injection-impl.md, sidecar-merge-design.md) and the repo's current state into a
single prioritized backlog. Items are grouped into tiers by impact and
readiness.

---

## Tier 1 — Low-hanging fruit (small effort, high polish)

These can each be done in a single focused session without new design work.

| # | Item | Source | Why now |
|---|------|--------|---------|
| 1.1 | ~~**Extend `--prompt` to stale/shifted/unreviewed diagnostics**~~ | prompt-mode-design.md | ✅ Done — `liyi check --prompt` now groups actionable diagnostics for `Stale`, `Shifted`, `Unreviewed`, and `ReqChanged` in addition to the original coverage-gap kinds. |
| 1.2 | ~~**Doc-comment detection for remaining languages**~~ | init-discover-impl.md (Phase 2 gap) | ✅ Done — 15/21 languages now have `doc_comment_detector`. Remaining 5 (Bash, Ruby not feasible; JSON/TOML/YAML not applicable). |
| 1.3 | ~~**Disambiguate Rust trait impls & ObjC categories in tree_path**~~ | tree_path resolver audit | ✅ Done — `impl Trait for Foo` now encodes as `impl."Trait for Foo"` (distinct from inherent `impl.Foo`); ObjC `@interface Foo (Cat)` encodes as `class."Foo (Cat)"`. Root cause of a mislabeled sidecar spec where two distinct impl blocks shared one tree_path. See item 2.8 for the remaining collision classes. |

> **Note on the 14 unreferenced requirements:** `liyi check` currently reports
> 7 requirements from lsp-design.md and 7 from sidecar-merge-design.md with no
> referencing item specs. This is expected — the code that would carry
> `@liyi:related` edges pointing to these requirements doesn't exist yet.
> They will be resolved naturally when the LSP (Tier 3) and sidecar merge
> (Tier 4.2) features are implemented.

## Tier 2 — Next milestones (moderate effort, unlocks downstream value)

### 2A. Approval workflow follow-through (v0.1.x scope)

| # | Item | Source |
|---|------|--------|
| 2.1 | ~~**StaleReviewed approval flow**~~ | approve-impl.md |
| 2.2 | ~~**ReqChanged approval flow**~~ | approve-impl.md |

**Status**: ✅ Done — `liyi approve` now surfaces unreviewed, stale-reviewed,
and requirement-changed items; the TUI shows source diffs for stale-reviewed
items and requirement diffs for req-changed items, and the CLI exposes
`--unreviewed-only`, `--stale-only`, and `--req-only` filters.

**Rationale**: This milestone is complete; keep it here as a shipped v0.1.x
checkpoint because later roadmap items build on the same review model.

### 2B. Library refactoring for LSP (Step 0)

| # | Item | Source |
|---|------|--------|
| 2.3 | ~~**Extract `build_requirement_registry()` as public API**~~ | lsp-design.md Step 0 |
| 2.4 | ~~**Export `RequirementRegistry` / `RequirementRecord`**~~ | lsp-design.md Step 0 |

**Status**: ✅ Done — `liyi::check` now exposes public `RequirementRegistry`
and `RequirementRecord`, plus `build_requirement_registry()` (pass 1) and
`check_sidecars()` (pass 2) that accept a prebuilt registry. `run_check()`
remains the CLI wrapper and is behavior-compatible. The
`lsp-check-refactor-exposes-registry` requirement is now referenced by
`build_requirement_registry`.

**Rationale**: This was the *prerequisite* gate for all LSP work — a pure
refactor with no behavioral change. With it landed, the LSP crate can cache
the registry across edits instead of re-scanning the project.

### 2C. VCS hints (Phase 3 of init-discover)

| # | Item | Source |
|---|------|--------|
| 2.5 | **`git log -L` per-span commit history** | init-discover-impl.md Phase 3 |
| 2.6 | **Fix-commit detection & test-presence heuristic** | init-discover-impl.md Phase 3 |
| 2.7 | **`--hints` flag gating** | init-discover-impl.md Phase 3 |

**Rationale**: Phase 3 is fully designed and was explicitly "deferred, not
cancelled." VCS hints significantly improve cold-start triage by telling agents
which items have churn or bug-fix history. The `git log -L` approach avoids the
git2 dependency. Can be worked in parallel with Tier 2B/2D.

### 2D. Resolve remaining tree_path name collisions

| # | Item | Source |
|---|------|--------|
| 2.8 | **Disambiguate same-named code siblings (overloads, reopened scopes)** | tree_path resolver audit |

**Background**: `resolve_segments` returns the *first* AST node matching a
`kind.name` pair, so any two sibling items that produce the same tree_path are
indistinguishable — the second is unaddressable and silently resolves to the
first. This was the root cause of a mislabeled sidecar spec (an inherent
`impl Diagnostic` carrying a trait impl's intent). The Rust trait-impl and
Objective-C category cases are fixed (item 1.3); a full audit of all 20 language
configs found these remaining collision classes:

| Language(s) | Collision | Frequency |
|---|---|---|
| C++, C#, Java, TypeScript | **Method/function overloading** — `add(int)` and `add(double)` both → `fn.add` | Medium — common in C++/Java/C# |
| C++ | **Reopened namespaces** — `namespace math {}` declared twice → `namespace.math` ×2 | Low–medium |
| Ruby | **Reopened classes/modules** (monkey-patching) → `class.Foo` ×2 | Low–medium |
| C# | **Partial classes** within one file → `class.Foo` ×2 | Low (rare in a single file) |

**Not affected**: Go and Ruby singleton methods already encode the receiver
type into the name; Python, JS/TS, Java, PHP, Kotlin, C, and the data-file
languages have unique names within scope.

**Design decision needed**: two viable approaches —
1. **Signature encoding** in `node_name` (the pattern used for Go receivers,
   Ruby singletons, Rust traits, ObjC categories): fold a disambiguating
   suffix (parameter types, namespace path, category) into the item name.
   Self-describing tree_paths, but the encoding is per-language and verbose.
2. **Sibling indexing**: extend the existing `name[N]` index syntax (currently
   data-file-only) to disambiguate same-named code siblings by position.
   Uniform across languages, but positional indices are brittle under edits
   and the existing reanchor logic would need to handle them.

Until resolved, overloaded/reopened items remain a latent mislabel risk. A
cheaper interim mitigation: have `liyi check` *detect and warn* when two specs
(or two discovered items) share a tree_path, surfacing the ambiguity even if it
can't auto-resolve it.

## Tier 3 — v0.2 headline: LSP server

Depends on Tier 2B completion.

| # | Item | Source | Phase |
|---|------|--------|-------|
| 3.1 | **Scaffold `liyi-lsp` crate** | lsp-design.md Step 1 | — |
| 3.2 | **Diagnostics & file watching** | lsp-design.md Step 2 | Phase 1 |
| 3.3 | **Code actions (reanchor, approve, scaffold)** | lsp-design.md Step 3 | Phase 2 |

**Rationale**: The LSP is the v0.2 headline feature and the largest unlock for
adoption — it brings real-time diagnostics to the editor. Phase 1 (diagnostics)
is the minimal viable LSP; Phase 2 (code actions) is the "comfortable" LSP.
Phases 3–4 (inlay hints, completions) are explicitly deferred past this.

## Tier 4 — Post-MVP (design-complete, build when ready)

These are fully designed but have lower urgency or wider blast radius. Sequence
by opportunity.

| # | Item | Source | Notes |
|---|------|--------|-------|
| 4.1 | **Triage workflow** (`liyi triage`) | liyi-design.md | Prompt assembly, validation, apply, summary. Zero LLM calls in binary. |
| 4.2 | **Sidecar auto-merge** | sidecar-merge-design.md | Three-way merge + field re-derivation. Becomes urgent once multi-contributor repos adopt liyi at scale. |
| 4.3 | **Additional injection profiles** (GitLab CI, K8s) | injection-impl.md | Core injection framework and GitHub Actions profile are already shipped; the remaining work is adding more profiles. Prioritize GitLab CI — second-largest CI platform. |
| 4.4 | **Challenge mode** | liyi-design.md | On-demand semantic verification. Blocked on LSP foundation (Tier 3). |
| 4.5 | **`liyi check --coverage`** | liyi-design.md | Compare discovered items vs existing specs. Infra exists; feature is deferred. |
| 4.6 | **`--json` output mode** | prompt-mode-design.md | Machine-readable output for dashboards and integrations. |
| 4.7 | **`liyi check --require-ignore-reason`** | liyi-design.md | Enforce justifications on `@liyi:ignore`. Convention exists; enforcement doesn't. |

## Tier 5 — Speculative / future-direction

Not designed in detail; captured for completeness.

| Item | Source |
|------|--------|
| Code-level dependency graph (`depends_on` field) | liyi-design.md |
| Workspace-aware requirement queries (monorepo) | liyi-design.md |
| `guarded_by` middleware tracking | liyi-design.md |
| LSP Phase 3–4 (inlay hints, completions, hover) | lsp-design.md |
| VS Code extension (separate repo) | lsp-design.md Step 4 |
| Batch `liyi init <directory>` | init-discover-impl.md |

---

## Suggested sequencing

```
Now          Tier 2B   (LSP library refactor)
             Tier 2C   (VCS hints)          ─── can parallelize ─── Tier 2D (tree_path collision work)
                │
v0.2         Tier 3.1 → 3.2 → 3.3  (LSP)          ← resolves 7 unreferenced lsp-design requirements
                │
Post-MVP     Tier 4 items by opportunity             ← 4.2 resolves 7 unreferenced merge-design requirements
```

Tier 1 is complete. Within Tier 2, 2B must precede Tier 3, while 2C and 2D are
independent of the LSP refactor and can be scheduled in parallel. Within Tier
4, items 4.1–4.3 are independent; 4.4 depends on Tier 3.
