<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
<!-- AIGC disclaimer: this document was drafted by an AI agent and has not been reviewed by the maintainer. -->

# Prioritized Next Steps

**As of**: 2026-03-17 · **Baseline**: v0.1.0, 275 specs current, 9 trivial, all tests and clippy clean.

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
| 1.1 | **Extend `--prompt` to stale/shifted/unreviewed diagnostics** | prompt-mode-design.md | The prompt-mode infra is shipped for coverage gaps; extending it to remaining diagnostic types is additive code, no new architecture. Greatly improves agent UX for the most common workflow. |
| 1.2 | **Doc-comment detection for remaining languages** | init-discover-impl.md (Phase 2 gap) | `_has_doc` hint works for 10/21 languages. Adding the remaining 11 (Ruby, Kotlin, Swift, Bash, Zig, …) is mechanical tree-sitter query work. Improves `liyi init` quality across ecosystems. |

> **Note on the 14 unreferenced requirements:** `liyi check` currently reports
> 7 requirements from lsp-design.md and 7 from sidecar-merge-design.md with no
> referencing item specs. This is expected — the code that would carry
> `@liyi:related` edges pointing to these requirements doesn't exist yet.
> They will be resolved naturally when the LSP (Tier 3) and sidecar merge
> (Tier 4.2) features are implemented.

## Tier 2 — Next milestones (moderate effort, unlocks downstream value)

### 2A. Complete the approval workflow (v0.1.x scope)

| # | Item | Source |
|---|------|--------|
| 2.1 | **StaleReviewed approval flow** | approve-impl.md |
| 2.2 | **ReqChanged approval flow** | approve-impl.md |

**Rationale**: These are the two remaining approval paths documented in
approve-impl.md. Without them, `"reviewed": true` items that drift have no
supported re-approval path. Both are fully designed; implementation is scoped to
approve.rs + TUI changes.

### 2B. Library refactoring for LSP (Step 0)

| # | Item | Source |
|---|------|--------|
| 2.3 | **Extract `build_requirement_registry()` as public API** | lsp-design.md Step 0 |
| 2.4 | **Export `RequirementRegistry` / `RequirementRecord`** | lsp-design.md Step 0 |

**Rationale**: This is the *prerequisite* gate for all LSP work. It's a pure
refactor with no behavioral change — low risk, high leverage. Doing it now
de-risks the v0.2 timeline.

### 2C. VCS hints (Phase 3 of init-discover)

| # | Item | Source |
|---|------|--------|
| 2.5 | **`git log -L` per-span commit history** | init-discover-impl.md Phase 3 |
| 2.6 | **Fix-commit detection & test-presence heuristic** | init-discover-impl.md Phase 3 |
| 2.7 | **`--hints` flag gating** | init-discover-impl.md Phase 3 |

**Rationale**: Phase 3 is fully designed and was explicitly "deferred, not
cancelled." VCS hints significantly improve cold-start triage by telling agents
which items have churn or bug-fix history. The `git log -L` approach avoids the
git2 dependency. Can be worked in parallel with Tier 2A/2B.

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
| 4.3 | **Additional injection profiles** (GitLab CI, K8s) | injection-impl.md | Mechanical once the framework is proven with GitHub Actions. Prioritize GitLab CI — second-largest CI platform. |
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
Now          Tier 1.1  (prompt-mode expansion)
             Tier 1.2  (doc-comment languages)
                │
Near-term    Tier 2A   (approval flows)  ─── can parallelize ─── Tier 2C (VCS hints)
             Tier 2B   (lib refactor)
                │
v0.2         Tier 3.1 → 3.2 → 3.3  (LSP)          ← resolves 7 unreferenced lsp-design requirements
                │
Post-MVP     Tier 4 items by opportunity             ← 4.2 resolves 7 unreferenced merge-design requirements
```

Tier 1 items are independent of each other and can be tackled in any order or
in parallel. Tier 2A and 2C are independent of each other but 2B must precede
Tier 3. Within Tier 4, items 4.1–4.3 are independent; 4.4 depends on Tier 3.
