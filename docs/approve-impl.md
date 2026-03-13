<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# `liyi approve`: Implementation Plan

**Status:** Partially implemented (unreviewed-item approval is shipped; requirement-change review is planned)
**Design authority:** `docs/liyi-design.md` — *reviewed-semantics*, *fix-never-modifies-human-fields*

---

## Motivation

<!-- @liyi:related reviewed-semantics -->

`liyi approve` is the human review gate. Agents infer intent, but only a human can confirm that an intent description is correct or that a code item still satisfies a changed requirement. Without this gate, agent-inferred specs accumulate unchecked, and requirement changes propagate silently.

---

## Design constraints

The following constraints are normative for the implementation.

<!-- @liyi:requirement reqchanged-orthogonal-to-reviewed -->
**ReqChanged is orthogonal to reviewed.** Accepting a requirement change (refreshing the related edge hash) is a separate axis from the item's `reviewed` status. A ReqChanged approval means "the item's intent still holds under the updated requirement" — it does not re-review the intent itself. `Yes` on a ReqChanged item updates only the related edge hash; it must not touch `reviewed`, `intent`, or `source_hash`.
<!-- @liyi:end-requirement reqchanged-orthogonal-to-reviewed -->

<!-- @liyi:requirement approve-never-approves-requirements -->
**Approve never approves requirements.** Requirements are authored by humans in design docs. They are the *input* to the review process, not its subject. `liyi approve` reviews intents (agent-inferred descriptions of code behavior) and related-edge staleness (whether code still satisfies changed requirements). Requirement text itself is never presented for approval — writing the requirement *is* the assertion; VCS provenance suffices.
<!-- @liyi:end-requirement approve-never-approves-requirements -->

<!-- @liyi:requirement reqchanged-demands-human-judgment -->
**ReqChanged always demands human judgment.** Even if a requirement change appears cosmetic (rewording with no semantic impact), the human must confirm. The point of liyi is that humans don't trust their own recall — "I'm sure this is fine" is exactly the failure mode liyi prevents. No auto-fix path exists for stale related edges; only `liyi approve` (or manual sidecar editing) can refresh them.
<!-- @liyi:end-requirement reqchanged-demands-human-judgment -->

---

## Status quo (implemented)

### What gets surfaced

`liyi approve` currently surfaces **unreviewed items** — specs where `reviewed` is `false` (or absent) and no `@liyi:intent` marker exists in source. These are typically agent-inferred intents that no human has confirmed.

### Candidate collection (`approve.rs`)

`collect_approval_candidates` walks the specified paths, resolves sidecar targets, and collects every `Spec::Item` where `reviewed == false`. For each candidate it gathers:

- **Item metadata:** name, intent text, source span, spec index
- **Full source file:** all lines, with span offset and length computed so the TUI can highlight the relevant section
- **Previous intent from Git:** `lookup_prev_intent` walks up to 20 commits via `git log` to find the most recent version where the item had `reviewed: true`, enabling diff display

An optional `--item` filter restricts candidates to a single item name.

### TUI (`tui_approve.rs`)

The interactive TUI presents one candidate at a time with three panes:

1. **Header:** item name, source file, line range, progress counter
2. **Intent pane:** shows the proposed intent text. If a previously approved intent exists, shows a word-level diff (using the `similar` crate with Patience algorithm) with red/green highlighting
3. **Source pane:** syntax-highlighted source code (via `syntect`) with the item's span visually distinguished. Scrollable with j/k/PgUp/PgDn

**Keybindings:**

| Key | Action |
|-----|--------|
| `y` | Approve — accept the intent as correct |
| `n` | Reject — mark `reviewed: false` explicitly |
| `s` / Enter | Skip — defer decision |
| `e` | Edit — open `$EDITOR` with the intent text pre-populated; comment block shows item context and previous intent. Approve with the edited text |
| `a` | Approve all remaining items |
| `b` / ← | Go back to previous item |
| → | Go forward without deciding |
| `q` / Esc | Quit — all undecided items remain as Skip |

### Decision application (`apply_approval_decisions`)

<!-- @liyi:related reviewed-semantics -->

Decisions are applied per-sidecar, grouped by file:

- **Yes:** set `reviewed = true`, recompute `source_hash` and `source_anchor`
- **Edit(text):** update `intent`, set `reviewed = true`, recompute hash/anchor
- **No:** set `reviewed = false`
- **Skip:** no mutation

Sidecar files are written back only if at least one mutation occurred. `--dry-run` suppresses all writes.

### Batch mode

When `--yes` is passed, or stdin is not a TTY, all candidates receive `Decision::Yes` automatically. This supports CI and scripted workflows where a human has pre-validated the intents.

---

## Planned: requirement-change review

### Problem

When a requirement's text changes, every item with a `related` edge pointing to that requirement becomes stale — `liyi check` reports `ReqChanged`. Currently the only way to resolve this is to manually update the related edge hash in the sidecar (set to `null` and run `--fix`, or compute the new hash). There is no human review step confirming that the item's intent and implementation still satisfy the updated requirement.

This is a gap: liyi exists precisely because developers cannot be trusted to "just know" that their code still satisfies a rephrased requirement. The tool should walk them through each affected item, show the requirement diff, and demand a judgment.

### Design

Extend `liyi approve` to surface **requirement-changed items** alongside unreviewed items. The same TUI, same decision loop, broader scope.

#### Candidate types

The `ApprovalCandidate` struct gains a discriminant (or a new sibling type) to distinguish two review modes:

1. **Unreviewed** (existing): agent-inferred intent, no human confirmation yet
2. **ReqChanged** (new): reviewed item whose related requirement text changed

For ReqChanged candidates, the candidate struct carries additional context:

- **Requirement name** and **old requirement text** (from the stored hash — recoverable via the requirement sidecar's git history or by diffing the old vs new source span)
- **New requirement text** (current source at the requirement's span)
- **The item's current intent** (which the human must evaluate against the new requirement)

#### TUI presentation for ReqChanged

The intent pane changes layout for ReqChanged candidates:

1. **Requirement diff:** old→new requirement text, word-level diff with red/green highlighting. Header identifies which requirement changed
2. **Item intent:** the item's current intent text. The question is: "does this intent still hold under the new requirement wording?"
3. **Source pane:** unchanged — shows the item's code for reference

#### Decisions for ReqChanged

| Key | Meaning for ReqChanged |
|-----|------------------------|
| `y` | Confirm — the item's intent still satisfies the updated requirement. Refresh the related edge hash to current |
| `n` | Reject — the item's intent or code needs rework. Leave the related edge stale as a todo marker |
| `s` | Skip — defer |
| `e` | Edit — update the item's intent to reflect the new requirement, then refresh the hash |

#### Application logic

<!-- @liyi:related fix-never-modifies-human-fields -->

For ReqChanged approvals:

- **Yes:** update the related edge hash for the changed requirement to the requirement's current `source_hash`. Do *not* touch `reviewed`, `intent`, or `source_hash` — they are orthogonal
- **Edit(text):** update `intent` to the new text, set `reviewed = false` (agent convention — the edited intent hasn't been reviewed in its final form yet, or `true` if the human authored it directly; this needs a policy decision), then refresh the related edge hash
- **No:** leave the related edge hash unchanged so `liyi check` continues to flag it
- **Skip:** no mutation

#### CLI surface

No new subcommand. `liyi approve` expands its scope:

```
liyi approve [paths...]
    --yes           Approve all without prompting
    --dry-run       Preview without writing
    --item NAME     Filter to specific item
    --req-only      Only show requirement-changed items (skip unreviewed)
    --unreviewed-only  Only show unreviewed items (current behavior)
```

Without `--req-only` or `--unreviewed-only`, both kinds are interleaved (unreviewed first, then req-changed, or sorted by file). The progress bar shows the combined count.

#### Ordering

Present unreviewed items first (these are typically fewer and higher-priority), then ReqChanged items grouped by requirement (so all items affected by the same requirement change appear together, with the requirement diff shown once as a group header).

### Implementation notes

- `collect_approval_candidates` gains a pass over `liyi check` diagnostics (or re-runs the check logic internally) to find `DiagnosticKind::ReqChanged` items. For each, it locates the item spec, the requirement name, and retrieves both old and new requirement text
- Old requirement text recovery: the simplest approach is `git show HEAD~1:<requirement-file>` or walking git history on the requirement's sidecar to find the last hash match. Alternatively, store the old hash and look up the requirement text at that hash — but requirement text isn't stored in sidecars, only the hash. Git history is the pragmatic path
- The `similar` diffing infrastructure already exists for intent diffs; requirement diffs reuse it
- `edit_intent_in_editor` needs a variant that also shows the requirement diff in the comment block

### Edge cases

- **Multiple requirements changed for one item:** the item appears once per changed requirement, or once with all changed requirements listed. The latter is more ergonomic — show all requirement diffs in the intent pane, single Yes/No refreshes all related edges
- **Requirement deleted:** this is already `UnknownRequirement` (error severity), not `ReqChanged`. `approve` doesn't handle it — the human must fix the `related` edge or the code
- **Requirement renamed:** currently appears as UnknownRequirement (old name) plus MissingRelatedEdge (new name, if `@liyi:related` was updated in source). Not handled by `approve` — manual sidecar edit

---

## Non-goals

- **Approving requirement text itself:** requirements are authored by humans in design docs. They don't go through `approve` — they're the *input* to the review process, not its subject
- **Auto-approving cosmetic changes:** even if a requirement change is cosmetic (rewording with no semantic impact), the human should still confirm. The point of liyi is that humans don't trust their own recall — "I'm sure this is fine" is exactly the failure mode liyi prevents
- **Batch `--yes` for ReqChanged:** this is allowed for consistency with unreviewed items, but should be used with care. It's equivalent to saying "I've read all the requirement changes and confirm all affected items are still valid" — a strong claim
