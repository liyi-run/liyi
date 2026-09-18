<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
<!-- AIGC: drafted with AI assistance (Claude Opus 4.8 via OpenCode); reviewed by a human maintainer before merge. -->

# `@liyi:note`: The Context Primitive

**Status:** ✅ Implemented (MVP: marker scanner + `liyi context <path>[:line]` resolver)
**Target:** v0.2
**Design authority:** this document
**Supersedes:** `docs/liyi-design.md` — *Module-level: `@liyi:module` marker*
**Related:** `docs/liyi-design.md` — *Self-hosting and the quine problem*,
*Multilingual annotations*; `docs/next-steps.md` (challenge mode, LSP)

---

## Summary

`@liyi:module` is replaced by `@liyi:note`, a **context primitive**. A *note* is
a block of governing prose — invariants, conventions, gotchas, architectural
context — that should be injected into an agent's (or reviewer's) working
context when they touch the code it governs.

The redesign reframes module-level intent away from the staleness engine. A note
is **marker-only and untracked**: it has no sidecar entry, no `source_hash`, no
`reviewed` flag, and is never seeded into adversarial unit tests. Its value is
*delivery* — getting the right prose in front of the right reader at the right
moment — not *verification*.

This is a **hard rename**, not an additive alias. `@liyi:module` is retired.
Pre-1.0 (currently v0.1), with no external adopters yet, the break is acceptable
and the migration is mechanical.

---

## Why the change

`@liyi:module` was specified as "module-level intent" but never did real work:
the linter only checked *presence* of the marker in a directory and never
consumed the prose. That left it in an awkward middle ground — it looked like a
tracked intent artifact (sibling to item specs and requirements) but carried
none of the machinery (no hash, no review, no staleness). Two problems followed:

1. **Wrong mental model.** Treating module prose as a *staleness* artifact
   invites the question "is this module spec stale?", which has no good answer:
   module invariants are broad and slow-moving, and hashing a whole README to
   gate it on every edit produces noise, not signal.
2. **Missed opportunity.** The genuinely valuable thing module prose can do is
   *inform the agent before it edits* — a retrieval/injection concern. Nothing
   in 立意 delivered that.

`@liyi:note` commits to the second framing and drops the first.

---

## Model

### A note is governing prose

A note is written wherever module documentation already lives — a `README.md`, a
`LIYI.md`, a Rust `//!` module doc, a Go `doc.go`, a Python module docstring —
using the host markup's comment syntax for the marker, exactly as `@liyi:module`
did. The prose is for humans, reviewers, and agents; the marker is the
machine-readable handle.

```markdown
# Billing

## 立意
<!-- @liyi:note billing-currency -->

All monetary amounts carry their currency. No function in this module silently
converts between currencies — mismatches must be explicit errors. Precision is
never lost through rounding without an explicit rounding parameter.

<!-- @liyi:end-note billing-currency -->
```

The optional name (`billing-currency`) is a **group label, not a unique ID**:
several blocks may share a name and are aggregated when that name is requested.
An unnamed note is anonymous and participates only by scope (below).

`@liyi:end-note` closes an embedded block so its extent is exact. It is
**required** when a note shares a file with other content (so the scanner knows
where the prose ends) and **optional** for a dedicated file whose entire body is
the note.

### `LIYI.md`: a dedicated note file

A directory may carry a `LIYI.md` whose purpose is to hold notes for that
subtree. The name is reserved — a file called `LIYI.md` is assumed to be ours,
so tools and reviewers can recognize it on sight without parsing. It is **not**,
however, magic: a `LIYI.md` still carries an explicit `@liyi:note` marker like
any other note-bearing file. The marker requirement is deliberate — it keeps the
scanning rule uniform (one gate, no per-filename special cases) and keeps the
data self-describing (the block announces what it is, even when copied out of its
file). `LIYI.md` is therefore a *convention for where notes live*, not a second
parsing path.

### Notes are untracked

This is the central design commitment.

<!-- @liyi:requirement note-is-untracked -->
**A note has zero sidecar footprint and is never hash-tracked.** Notes are
discovered by a live scan of source/doc files at the moment context is
requested. A note has no `.liyi.jsonc` entry, no `source_hash`, no
`source_span` record, no `reviewed` flag, and no authorship metadata. It is
therefore never reported as STALE, never gates an exit code, and is never used
to seed adversarial unit tests. Provenance and review of note prose are handled
by ordinary version control and code review, not by 立意's staleness machinery.
This is what distinguishes a note from an item spec or a requirement: those are
hash-anchored claims subject to staleness; a note is delivered context subject
only to being found.
<!-- @liyi:end-requirement note-is-untracked -->

### Scope: where a note applies

<!-- @liyi:requirement note-directory-scope -->
**A note's default scope is its directory subtree, shadowed by nesting, and only
marker-bearing files participate.** A note applies to the directory it lives in
and all descendant directories, unless a deeper directory provides its own note,
which shadows the ancestor's for that subtree (nearest-note-wins, like
`.gitignore` cascading but resolved by proximity). Only files that carry an
explicit `@liyi:note` marker — typically a `README`'s 立意 section or a dedicated
`LIYI.md` — enter the cascade; ordinary documentation with no marker is invisible
to context resolution. This gate is what keeps the implicit cascade controlled: a
project-root `README` of fifty pages of user manual contributes nothing to a
feature's working context unless a `@liyi:note` block explicitly opts a portion
of it in. Scope determines which notes are *candidates* for a given file's
context; it does not by itself rank or filter them beyond shadowing. A file's
applicable notes are therefore the nearest marked note in each ancestor chain
plus any notes explicitly pulled in by an `@liyi:see` reference (see
`note-see-membership`). Scope is computed live from the directory tree at query
time; it is never frozen into a sidecar.
<!-- @liyi:end-requirement note-directory-scope -->

### Membership: pulling a note into a location

Directory scope is coarse. `@liyi:see <name>` lets a specific item opt into a
named note regardless of where that note lives — a cross-cutting invariant
(e.g. a security or money-handling note) can be attached precisely to the
functions that must honor it.

```rust
// @liyi:see money-rounding
fn settle(invoice: &Invoice) -> Result<Receipt, Error> { ... }
```

<!-- @liyi:requirement note-see-membership -->
**`@liyi:see <name>` is a live, multi-valued, opt-out retrieval hint that is
never stored in a sidecar.** An item may carry multiple `@liyi:see` references
to pull several named notes into its context, and the same note may be `see`-n
from many items. References are resolved by live scan at query time, exactly
like notes themselves — they produce no `.liyi.jsonc` entry and no hash. A
`see` reference to a name that matches no note is a recoverable diagnostic
(warning), never a hard error, because retrieval is best-effort by design: a
missing note degrades context, it does not break a build. An opt-out sentinel
(`@liyi:see =none`) suppresses otherwise-in-scope notes for an item that should
not inherit them. Because `see` is a hint and not a tracked claim, it never
emits staleness and never gates an exit code.
<!-- @liyi:end-requirement note-see-membership -->

---

## Two graphs

The redesign rests on separating two graphs that `@liyi:module` blurred.

<!-- @liyi:requirement two-graph-separation -->
**立意 maintains two distinct graphs with different guarantees.** The
*staleness graph* connects hash-anchored claims — item specs and requirements,
linked by `@liyi:related` edges — and is the basis for STALE/REQ-CHANGED
detection and exit codes. The *retrieval graph* connects notes to locations via
directory scope and `@liyi:see` membership; it is best-effort, untracked, emits
no staleness, and never gates trust or exit codes. The two never cross-
contaminate: a note can reference a requirement by name as a retrieval
convenience (an untracked `@liyi:related` from note prose is a pointer, not a
tracked edge), but doing so does not pull the note into the staleness graph, and
a stale requirement never marks a note stale. Keeping the graphs separate
prevents the "everything depends on everything" collapse, in which broad context
prose, hash-gated, would make every edit report spurious staleness.
<!-- @liyi:end-requirement two-graph-separation -->

| | Staleness graph | Retrieval graph |
|---|---|---|
| Nodes | item specs, requirements | notes |
| Edges | `@liyi:related` (tracked) | directory scope, `@liyi:see` |
| Anchoring | `source_hash` | none (live scan) |
| Failure mode | STALE / REQ-CHANGED, exit 1 | warning at most |
| Sidecar footprint | required | none |
| Purpose | verify intent didn't drift | deliver context to the reader |

---

## Choosing between a note and a requirement

Notes and requirements can both hold a paragraph of governing prose, so authors
need a rule for which channel a given paragraph belongs in. The rule is about the
**direction of derivation**, not the importance of the text:

- A **requirement** is prescriptive: the code is a *derivation* of the text. The
  text is the authority; the implementation exists to satisfy it. Edits to the
  text legitimately warrant re-verifying every item that derives from it. This is
  why a requirement is hash-tracked and why `@liyi:related` edges to it gate exit
  codes.
- A **note** is descriptive: the text is a *non-binding description* of code that
  is itself the authority. The prose helps a reader understand or safely modify
  the code, but the code does not exist to satisfy the prose. This is why a note
  is untracked — there is nothing to verify drift against.

Three practical tests make the call concrete:

1. **The re-approval smell test.** Imagine editing the paragraph. If the prospect
   of `liyi check` then reporting ten to fifty `REQ-CHANGED` items — each
   demanding re-review — feels like *noise* rather than *the right thing to do*,
   the paragraph is a note. Broad, slow-moving context that governs a whole
   subtree almost always fails this test: hash-gating it manufactures churn
   without signal. This test is self-reinforcing in practice — if a tracked
   requirement keeps producing `REQ-CHANGED` reports you instinctively dismiss,
   that is the tool telling you the text should have been a note.
2. **The provenance test.** If the paragraph reads like a theorem, an axiom, or
   an acceptance criterion — or it originated in a high-stakes design review, an
   industry standard, or an organizational specification — it carries external
   authority that the code must honor. That is a requirement; track it so drift
   is caught.
3. **The substitution test.** Ask "is the code a derivation of this text, or is
   this text a description of the code?" The first is a requirement; the second
   is a note.

The two channels are **not mutually exclusive**. A formally-authored
requirement can also be referenced informatively, via `@liyi:see`, by functions
that are not direct derivations but should nonetheless be aware of it — the same
prose then serves the staleness graph (for its derivations) and the retrieval
graph (for its bystanders). This overlap is an edge case, not the common path:
most prose is cleanly one or the other.

---

## Consumption

A note is only as useful as the surface that delivers it. The near-term consumer
is a CLI query; richer surfaces follow.

<!-- @liyi:requirement context-resolution -->
**Context resolution for a location returns the applicable notes, deterministic-
ally ordered, by live scan.** Given a location (a file, or a `path:line`), 立意
resolves the set of applicable notes — the nearest in-scope note in each
ancestor chain (per `note-directory-scope`), plus all notes named by the
location's `@liyi:see` references (per `note-see-membership`), minus any
suppressed by an opt-out sentinel. Notes sharing a name are aggregated. The
result is ordered deterministically by `(source path, start line)` so that
output is stable across runs and reviewable in diffs. Resolution reads the
working tree live; it consults no sidecar and computes no hash. This is the
contract the `liyi context <path:line>` command (and later LSP/MCP context
surfaces) is built on.
<!-- @liyi:end-requirement context-resolution -->

### `liyi context <path:line>` (MVP)

The first consumer is a read-only command that prints the resolved notes for a
location — usable by an agent before editing, or by a human in review:

```text
$ liyi context crates/liyi/src/money.rs:42
# billing-currency  (crates/liyi/src/README.md)
All monetary amounts carry their currency. No function in this module silently
converts between currencies — mismatches must be explicit errors.

# money-rounding  (via @liyi:see)  (docs/invariants/money.md)
Rounding requires an explicit mode argument; banker's rounding is the default.
```

### Later surfaces (deferred)

- **LSP / MCP context API.** The same resolution contract exposed as an editor
  hover / agent tool call, so context arrives without an explicit command. This
  is the eventual home (cross-ref `docs/next-steps.md`, LSP tier).
- **Challenge mode.** Verifying that code actually honors note prose is a
  *challenge*-style check (deferred, LSP-dependent), not an adversarial unit
  test. Notes never seed unit tests — their breadth makes generated assertions
  noisy. See `docs/next-steps.md` (challenge mode).

---

## Migration from `@liyi:module`

Mechanical, pre-1.0, no external adopters:

1. `@liyi:module` → `@liyi:note` (optionally add a name).
2. Add `@liyi:end-note` where a note shares a file with other content.
3. No sidecar changes — notes were never in sidecars, and `@liyi:module` carried
   no tracked state to migrate.

The JSON schema is **unchanged**: notes have no sidecar representation, so no
`moduleSpec`/`noteSpec` object and no `in_modules`/`in_notes` field is added.

---

## Implementation surface (non-normative)

- `crates/liyi/src/markers.rs`: replace the `Module` variant with
  `Note { name: Option<String>, line }`, add `EndNote { name, line }` and
  `See { name, line }`; update the marker keyword list (escaped `\x40`),
  hard-removing `@liyi:module` in favor of `@liyi:note`.
- New resolution module: directory-scope walk + `@liyi:see` aggregation, live,
  no sidecar reads.
- New `liyi context <path:line>` subcommand built on the resolution contract.
- `crates/liyi/src/check.rs`: the staleness engine is **untouched** — notes do
  not enter it. Remove the presence-only `@liyi:module` directory check.

This document is the design authority; the scheduled implementation plan will
track requirement coverage (see `docs/next-steps.md`).
