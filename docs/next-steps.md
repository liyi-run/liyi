<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
<!-- AIGC disclaimer: this document was drafted by an AI agent and has not been reviewed by the maintainer. -->

# Prioritized Next Steps

**As of**: 2026-05-30 · **Baseline**: v0.1.0, prompt-mode expansion and approval workflow shipped; doc-comment detection complete for all feasible languages.

This document synthesizes the existing roadmaps (liyi-design.md, lsp-design.md,
init-discover-impl.md, prompt-mode-design.md, injection-impl.md,
file-directive-design.md, note-context-design.md, sidecar-merge-design.md) and
the repo's current state into a single prioritized backlog. Items are grouped
into tiers by impact and readiness.

---

> **Note on unreferenced requirements:** `liyi check` currently reports
> requirements in several design documents (notably lsp-design.md,
> sidecar-merge-design.md, file-directive-design.md, and
> note-context-design.md) that have no referencing item specs. This is expected
> — the code that would carry `@liyi:related` edges pointing to these
> requirements doesn't exist yet. They will be resolved naturally when the
> corresponding features (Tiers 2–4) are implemented.

## Tier 2 — Next milestones (moderate effort, unlocks downstream value)

### 2C. VCS hints (Phase 3 of init-discover)

| # | Item | Source |
|---|------|--------|
| 2.5 | **`git log -L` per-span commit history** | init-discover-impl.md Phase 3 |
| 2.6 | **Fix-commit detection & test-presence heuristic** | init-discover-impl.md Phase 3 |
| 2.7 | **`--hints` flag gating** | init-discover-impl.md Phase 3 |

**Rationale**: Phase 3 is fully designed and was explicitly "deferred, not
cancelled." VCS hints significantly improve cold-start triage by telling agents
which items have churn or bug-fix history. The `git log -L` approach avoids the
git2 dependency. Can be worked in parallel with Tier 2D/2E.

### 2D. Resolve remaining tree_path name collisions

| # | Item | Source |
|---|------|--------|
| 2.8 | **Disambiguate same-named code siblings (overloads, reopened scopes)** | tree_path resolver audit |

**Background**: `resolve_segments` returns the *first* AST node matching a
`kind.name` pair, so any two sibling items that produce the same tree_path are
indistinguishable — the second is unaddressable and silently resolves to the
first. This was the root cause of a mislabeled sidecar spec (an inherent
`impl Diagnostic` carrying a trait impl's intent). The Rust trait-impl and
Objective-C category cases are already fixed; a full audit of all 20 language
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

### 2E. `@liyi:file` file-scoped directives

| # | Item | Source |
|---|------|--------|
| 2.9 | **`@liyi:file language=` override + discovery-level `ignore`** | `docs/file-directive-design.md` |

A single namespaced directive family for file-scoped metadata. `language=<lang>`
overrides language detection (precedence: inline > injection profile > extension),
fixing extension-ambiguous cases like `.h` (C vs C++) and extensionless files.
`ignore` excludes a file from discovery additively with `.liyiignore`, for files
you own. Implementation surface: `markers.rs` (new `File` variant + `key=value`
parse), `detect_language_with_meta(path, content)` in `tree_path/mod.rs` (and
retire the stale `.h`→C doc comment), and a discovery hook alongside the
`.liyiignore` cascade. Independent of the LSP work; can parallelize. Design
authority: `docs/file-directive-design.md` (requirements `file-directive-namespace`,
`file-language-precedence`, `file-ignore-additive`).

## Tier 3 — v0.2 headline: LSP server

Builds on the already-landed `liyi::check` library surface, so the server can
cache the requirement registry across edits.

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
Now          Tier 2C  (VCS hints)    ─── can parallelize ─── Tier 2D  (tree_path collision work)
             Tier 2E  (@liyi:file directives)
                │
v0.2         Tier 3.1 → 3.2 → 3.3  (LSP)
                │
Post-MVP     Tier 4 items by opportunity
```

Tier 1 is complete and the Tier 2 LSP library refactor has landed, unblocking
Tier 3. The remaining Tier 2 items (2C, 2D, 2E) are independent of the LSP
refactor and can be scheduled in parallel. Within Tier 4, items 4.1–4.3 are
independent; 4.4 depends on Tier 3.
