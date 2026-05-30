<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Throwaway Roadmap: Streamlining `liyi-design.md`

**Status:** temporary planning note, intended to be deleted once the
streamlining commits land. Not part of the published design.

**Goal:** `docs/liyi-design.md` went through many rounds of multi-model
adversarial review. That process improved correctness but left behind
duplicated reasoning, three competing milestone vocabularies, and
revision-history narration embedded in the prose. This roadmap catalogues
the cleanups and the order in which to apply them — **one commit per logical
change**.

**Non-goal:** No design *decision* changes. This is editorial: remove
redundancy and dated framing, keep every substantive claim. If a cut would
drop information that appears nowhere else, it is out of scope.

---

## Milestone model (context for the vocabulary cleanup)

The release model the doc should reflect:

- **0.1.0** = the MVP (already built) plus the in-progress LSP-library
  refactor, scope-cut so that LSP itself and everything else lands later.
- **0.2.0+** = one single "later" bucket, catalogued in `docs/next-steps.md`.

Because the whole document *is* the 0.1.0 design, "currently" is implied
everywhere; most "in 0.1" hedges add nothing.

### Canonical word: `Deferred`

Replace the three overlapping schemes — `MVP`/`post-MVP`,
`0.1`/`post-0.1`/`0.2+`/`0.x`, and `future direction`/`later release` — with
a single grep-friendly token: **`Deferred`**.

Rationale:

- Distinctive for `grep -ni deferred` — gives a near-complete inventory of
  out-of-scope items.
- Reads as a deliberate scope decision, not an unfinished section (unlike
  `TODO`) and not a grep-noisy word (unlike `Future`).
- Already in use: the "Deferred languages" table standardizes on it.

Tone refinement (prose only, single grep token in both):

- `Deferred to 0.2.0` — concrete next-steps items (LSP, MCP, triage,
  challenge, `--json`).
- `Deferred (speculative)` — may never ship (`guarded_by`, `depends_on`
  code-dependency graph, `liyi import-reqif`).

### Keep literal version numbers only where they are data

- The schema `"version": "0.1"` field in every example sidecar.
- The `liyi migrate` discussion (0.1 → 0.2 transitions, "additive in 0.x").
- JSON Schema `$id` URLs (`.../schema/0.1/...`).
- `Appendix: tree_path Grammar Specification (v0.3)` — a real spec version.
  (But trim the inline "v0.2 grammar used … v0.3 eliminates" narration to a
  one-line note.)

### Drop bare "in 0.1" hedges

Examples to simplify where the sentence stands without the qualifier:

- "Span-shift detection (included in 0.1)" → "Span-shift detection".
- "Built-in languages (21 in 0.1)" → "Built-in languages" (count ages badly).
- "No other hash algorithm is supported in 0.1" → "No other hash algorithm is
  supported".
- "for 0.1 the convention-based approach … is sufficient" → drop "for 0.1".

---

## Revision-history narration to strip

These read like changelog entries embedded in a spec. Fold the content into
present tense; delete the dated/versioned framing.

- `# 立意 (Lìyì) — Design v8.12` header — drop the `v8.12`.
- `**v8.4 update:**` in Risk #4 — fold into present-tense prose.
- `**Generalization update (v0.1.x).**` (Annotation-coverage section) and the
  echoing `**Generalization update.**` (cognitive-load section) — state the
  present design ("`--prompt` covers stale items, shifted spans, unreviewed
  specs, requirement-changed items"); drop the "used to be narrower" framing.
- Dated parentheticals in the Risks section: `(updated 2026-03-06)`,
  `(added 2026-03-11)`, `(added 2026-05-29)`, `(2026-03-31)` feasibility-probe
  date — drop the dates, keep the content.

Leave alone: the AIGC Disclaimer's model list (it is a compliance artifact),
and the "Level 0–6" adoption ladder (a conceptual sequence, not a timeline).

---

## Content-redundancy cuts (one commit each)

Numbered to match the discussion. Items 1–8 are high-confidence; 9–12 are
softer calls where local self-containment may be worth keeping.

### 1. Knowledge-systems decay litany

The Redmine→JIRA / read-only Confluence / superseded design-docs list appears
~5 times: "The Idea" para 3, "Origin", "Adoption Story / The problem",
"Selling points / Outlives your knowledge systems", plus echoes in "Who this
is for" and the JIRA/Confluence comparison.

**Plan:** tell it once with full color in "Origin"; reduce the others to one
sentence + cross-reference. (~25 lines)

### 2. "Frameworks declare infrastructure; 立意 captures business logic"

Stated in full ~5 times: "The Idea" para 4, "Cross-cutting concerns" intro,
"Adoption Story / The problem", "Why this and not X / vs. my framework",
"Selling points / Captures the gap". The "refund if captured within 7 days"
example recurs verbatim.

**Plan:** keep the canonical statement in "The Idea" and the focused
application in "Cross-cutting concerns"; compress the three marketing
restatements to one-liners. (~15 lines)

### 3. Doubled opening pitch

The two consecutive opening paragraphs ("AI writes your code…" /
"AI agents write most code…") make the same point.

**Plan:** merge into one paragraph. (~4 lines)

### 4. "`liyi` never calls an LLM / binary is index, agent is brain"

Repeated as a standalone principle in: Triage "Architectural principle",
"Why the LLM is not in the binary", Challenge section, "What This Is Not".

**Plan:** state once with rationale in the Triage section; have the others
reference it. (~12 lines)

### 5. Augment Intent five-axis comparison

The same five axes (durability, trust model, CI enforcement, adversarial
testing, vendor neutrality) appear in both "vs. Augment Code's Intent" and
Risk #2 "Platform competition".

**Plan:** make Risk #2 authoritative (it has the table + funding context);
shrink the "vs. Augment" bullet to a summary + cross-reference. (~15 lines)

### 6. Risk #6 restates the cognitive-load-inversion section

Both cover the same five AGENTS.md problems and the MUST/SHOULD/REFERENCE
remedy.

**Plan:** Risk #6 states the *risk* (weak models forget rules) and points to
the cognitive-load section for the *solution*. (~20 lines)

### 7. Two-review-paths trust table

The friction/conspicuousness tradeoff table appears in both "Source-Level
Intent" and "Security Model / Two paths, two trust profiles".

**Plan:** keep mechanics in "Source-Level Intent", threat analysis in
"Security Model"; remove the duplicated table from one. (~10 lines)

### 8. tree_path vs. span-shift brittleness

The "add an import, 20 specs go stale" explanation + tree_path-as-mitigation
story appears in "Per-item staleness", "Structural identity via tree_path",
and Risk #4.

**Plan:** compress Risk #4 to a cross-reference + residual-friction note.
(~10 lines)

### 9–12. Softer / optional cuts

- **9. Adoption overlap:** "Progressive adoption" table, "Intended workflow",
  and "Migrating an existing project" restate the same level-by-level story;
  the brownfield `_hints`/`liyi init` walkthrough appears twice.
- **10. `--prompt` generalization notes:** consolidate the duplicate
  "Generalization update" paragraphs (overlaps with the revision-history
  strip above).
- **11. `=doc`/`=trivial` sentinel tables:** the "Who/Form/Meaning" and "When
  to use which" tables overlap with the prose preceding each.
- **12. Self-reference/quine:** "The self-reference is not accidental"
  paragraph restates the dogfooding point made in "Self-hosting" and again in
  "Success criteria / Dogfooding".

Decide per-item whether self-containment is worth the duplication.

---

## Commit sequence

1. `docs(design): unify milestone vocabulary on "Deferred"` (versioning).
2. `docs(design): strip revision-history narration from spec prose`.
3. `docs(design): tell knowledge-decay story once (item 1)`.
4. `docs(design): deduplicate framework-gap framing (item 2)`.
5. `docs(design): merge doubled opening pitch (item 3)`.
6. `docs(design): consolidate "liyi never calls an LLM" principle (item 4)`.
7. `docs(design): make Risk #2 the canonical Augment comparison (item 5)`.
8. `docs(design): fold Risk #6 into cognitive-load section (item 6)`.
9. `docs(design): deduplicate two-review-paths trust table (item 7)`.
10. `docs(design): compress tree_path brittleness in Risk #4 (item 8)`.
11. (optional) items 9–12, one commit each, after review.

Each commit carries the required AIGC trailers. After the series lands, delete
this file.
