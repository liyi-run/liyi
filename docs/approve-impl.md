<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# `liyi approve`: Implementation Plan

**Status:** Partially implemented (unreviewed-item approval is shipped; requirement-change and stale-reviewed approval are planned)
**Design authority:** `docs/liyi-design.md` — *reviewed-semantics*, *fix-never-modifies-human-fields*, *fix-semantic-drift-protection*

---

## Motivation

<!-- @liyi:related reviewed-semantics -->

`liyi approve` is the human review gate. Agents infer intent, but only a human can confirm that an intent description is correct, that a code item still satisfies a changed requirement, or that a previously reviewed item's intent still holds after the source code changed. Without this gate, agent-inferred specs accumulate unchecked, requirement changes propagate silently, and stale reviewed items linger with no structured path back to a clean state.

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

<!-- @liyi:requirement stale-reviewed-demands-human-judgment -->
**Stale reviewed specs always demand human judgment.** When a reviewed item's source code changes, `liyi check --fix` refuses to auto-rehash — the spec remains stale until a human confirms via `liyi approve` (or manual sidecar editing) that the declared intent still describes the changed code. This is the complement of the ReqChanged gate: ReqChanged guards against requirement drift; StaleReviewed guards against implementation drift. No auto-fix path exists for stale reviewed specs.
<!-- @liyi:end-requirement stale-reviewed-demands-human-judgment -->

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

The `ApprovalCandidate` struct gains a discriminant (or a new sibling type) to distinguish three review modes:

1. **Unreviewed** (existing): agent-inferred intent, no human confirmation yet
2. **ReqChanged** (new): reviewed item whose related requirement text changed
3. **StaleReviewed** (new): reviewed item whose source code changed (`source_hash` mismatch, flagged by `liyi check` as "not auto-rehashed — reviewed")

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
    --stale-only    Only show stale-reviewed items
```

Without `--req-only`, `--unreviewed-only`, or `--stale-only`, all three kinds are interleaved (unreviewed first, then stale-reviewed, then req-changed). The progress bar shows the combined count.

#### Ordering

Present unreviewed items first (these are typically fewer and higher-priority), then StaleReviewed items sorted by file (so the human walks through changed code in file order), then ReqChanged items grouped by requirement (so all items affected by the same requirement change appear together, with the requirement diff shown once as a group header).

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

## Planned: stale-reviewed approval

### Problem

When a reviewed item's source code changes, `liyi check --fix` updates the `source_span` (if the item shifted) but refuses to rehash `source_hash` — the "not auto-rehashed — reviewed" diagnostic. The spec remains stale until a human confirms the intent still holds. Currently the only resolution paths are:

1. Manual sidecar editing (set `source_hash` to `null`, run `--fix`)
2. Agent triage (write a triage report, run `liyi triage --apply` for cosmetic changes)

Neither path provides the ergonomic, interactive review that `liyi approve` offers for unreviewed items. A human looking at `liyi check` output with several "not auto-rehashed — reviewed" warnings has no structured way to walk through each one, compare old code vs new code, and confirm or update the intent.

### Design

Extend `liyi approve` to surface **stale reviewed items** alongside unreviewed and ReqChanged items. The same TUI, same decision loop, broader scope.

<!-- @liyi:related stale-reviewed-demands-human-judgment -->
<!-- @liyi:related fix-semantic-drift-protection -->

#### Candidate collection

`collect_approval_candidates` gains a pass over `liyi check` diagnostics (or re-runs the check logic internally) to find `DiagnosticKind::Stale` items where `reviewed == true` (or `@liyi:intent` in source). For each, it collects:

- **Item metadata:** name, intent text, source span, spec index
- **Current source:** all lines, with the item's span highlighted
- **Old source from Git:** the source lines at the span recorded in the stale `source_hash`, recovered via `git show <commit>:<file>` or by walking git history to find the commit whose content matches the recorded hash. This enables a source diff
- **Current intent:** the human-reviewed intent that may or may not still apply

#### TUI presentation for StaleReviewed

The intent pane changes layout for StaleReviewed candidates:

1. **Header:** item name, source file, line range, label "STALE — source changed since last review"
2. **Source diff:** old source (at time of last hash) → new source (current span), word-level or line-level diff with red/green highlighting. This is the key information — the human needs to see *what changed* in the code
3. **Intent pane:** the item's current intent text, displayed in full. The question is: "does this intent still describe the changed code?"
4. **Source pane:** full syntax-highlighted source code of the item's current span, scrollable

#### Decisions for StaleReviewed

| Key | Meaning for StaleReviewed |
|-----|---------------------------|
| `y` | Confirm — the intent still describes the changed code. Rehash `source_hash` and `source_anchor` to current, keep `reviewed: true` |
| `n` | Reject — the intent no longer holds. Leave stale as a todo marker |
| `s` | Skip — defer |
| `e` | Edit — update the intent to match the new code, then rehash. Set `reviewed: true` (the human authored the new intent directly) |

#### Application logic

<!-- @liyi:related fix-never-modifies-human-fields -->
<!-- @liyi:related reviewed-semantics -->

For StaleReviewed approvals:

- **Yes:** recompute `source_hash` and `source_anchor` from the current source span. Keep `reviewed = true` and `intent` unchanged — the human confirmed the existing intent still holds
- **Edit(text):** update `intent` to the new text, recompute `source_hash` and `source_anchor`, set `reviewed = true` (the human directly authored the replacement intent)
- **No:** leave `source_hash` unchanged so `liyi check` continues to flag it as stale
- **Skip:** no mutation

### Implementation notes

- Source diff recovery: `lookup_prev_source` (new function, parallel to `lookup_prev_intent`) walks git history to find the source file content at the commit where the item's `source_hash` last matched. Falls back to showing just the current source if git history is unavailable
- The `similar` crate's line-level diff (already used for intent diffs) works for source diffs. Word-level diff may be too noisy for code; line-level with inline change highlighting is likely more readable
- `edit_intent_in_editor` reuses the existing flow but includes the source diff and current intent in the comment block

### Edge cases

- **Item both stale and ReqChanged:** the item appears in both categories. Present it as StaleReviewed first (the source change is the more fundamental concern); if the human approves the intent, the ReqChanged review follows. Alternatively, combine into a single review showing both the source diff and the requirement diff — but this risks information overload. Presenting separately is safer
- **`@liyi:intent` in source:** if the item has `@liyi:intent` in source (making it effectively reviewed), and the source changed, it should still surface as StaleReviewed. The source-level intent assertion may no longer match the code. `liyi check` already flags these as stale
- **Stale but cosmetically:** the source hash changed but the behavioral semantics didn't (e.g., variable rename, comment edit). The human sees the diff, confirms it's cosmetic, and approves. No shortcut — the whole point is that the human judges, not the tool
- **`--yes` batch mode:** allowed for consistency, but carries the same caution as ReqChanged batch approval — the human claims to have verified all stale intents still hold, which is a strong assertion

---

## Non-goals

- **Approving requirement text itself:** requirements are authored by humans in design docs. They don't go through `approve` — they're the *input* to the review process, not its subject
- **Auto-approving cosmetic changes:** even if a requirement change is cosmetic (rewording with no semantic impact), the human should still confirm. The point of liyi is that humans don't trust their own recall — "I'm sure this is fine" is exactly the failure mode liyi prevents
- **Batch `--yes` for ReqChanged:** this is allowed for consistency with unreviewed items, but should be used with care. It's equivalent to saying "I've read all the requirement changes and confirm all affected items are still valid" — a strong claim
