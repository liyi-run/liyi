# 立意 (Lìyì) — 0.1.x Roadmap

2026-03-06 (updated 2026-03-09)

---

## Overview

This document covers post-MVP work that ships as 0.1.x patch releases. Everything here is additive — no schema changes, no CLI breaking changes, no behavioral regressions. Users who never enable a Cargo feature or run a new subcommand see zero impact.

The MVP roadmap (`docs/liyi-mvp-roadmap.md`) covers the 0.1.0 release. This document picks up where it leaves off.

**Design authority:** `docs/liyi-design.md` v8.7 — see *Structural identity via `tree_path`*, *Multi-language architecture (`LanguageConfig`)*, and *Annotation coverage*.

---

## Current Status Summary

| Milestone | Status | Notes |
|-----------|--------|-------|
| M3 Remaining MVP gaps | ✅ Complete | All items implemented |
| M5.1 MissingRelated | ✅ Complete | Diagnostic implemented, auto-fix in `--fix` mode |
| M5.2 `--fail-on-untracked` | ✅ Complete | Flag implemented with tests |
| M5.4 Golden fixtures | ✅ Complete | `missing_related/` and `missing_related_pass/` added |
| M5.5 AGENTS.md rule 11 | ✅ Complete | Pre-commit check requirement added |
| M5.3 `--prompt` mode | ⏳ Design | Design doc at `docs/prompt-mode-design.md` |
| M6.1–M6.3 NL-quoting core | ✅ Complete | Fenced blocks, inline backticks, quote chars |
| M6.4 `.liyiignore` cleanup | ✅ Complete | docs/ removed from ignore |
| M6.5 AGENTS.md escape | ✅ Complete | Unicode escape for @ in JSON |
| M6.6 Tests | ✅ Complete | Unit tests for NL-quoting |
| M6.7 Contributing guides | ✅ Complete | NL-quoting documented |

---

## M1. Multi-language `tree_path` support

**Status:** Not started — deferred to post-0.1.x or community contribution.

**Goal:** Extend tree-sitter-based structural identity from Rust-only to Python, Go, JavaScript, and TypeScript.

**Prerequisite:** Refactor `tree_path.rs` from hardcoded Rust-specific `KIND_MAP` + `node_name` to a data-driven `LanguageConfig` abstraction. This is the enabling refactor — each subsequent language is additive data, not new code paths.

### M1.1. `LanguageConfig` refactor (~half day)

Extract the four language-specific touch points into a configuration struct:

| Current code | Becomes |
|---|---|
| `KIND_MAP` (hardcoded Rust node kinds) | `LanguageConfig::kind_map` |
| `Language` enum (only `Rust`) | Extended with variants per feature |
| `detect_language()` (only `.rs`) | Dispatch table from extensions |
| `make_parser()` (only `tree_sitter_rust`) | `LanguageConfig::ts_language` |
| `node_name()` (`impl_item` special case) | `LanguageConfig::name_overrides` |

The `LanguageConfig` struct (from design doc v8.6):

```rust
struct LanguageConfig {
    ts_language: tree_sitter::Language,
    extensions: &'static [&'static str],
    kind_map: &'static [(&'static str, &'static str)],
    name_field: &'static str,
    name_overrides: &'static [(&'static str, &'static str)],
    body_fields: &'static [&'static str],
}
```

**Acceptance criteria:**
- All existing tests pass with Rust handled via `LanguageConfig` instead of hardcoded paths.
- Adding a new language requires only a new `LanguageConfig` constant and a Cargo feature — no changes to resolve/compute logic.

### M1.2. Python (`lang-python` feature)

**Grammar:** `tree-sitter-python` (0.25.0)

**Kind mappings:**

| Shorthand | Node kind |
|---|---|
| `fn` | `function_definition` |
| `class` | `class_definition` |

**Design notes:**
- Methods are `function_definition` inside `class_definition` body. Tree_path: `class::MyClass::fn::my_method`.
- No `impl` blocks — methods are direct children of the class body.
- Decorators (`@staticmethod`, `@app.route`) are siblings, same as Rust attributes — existing `find_item_in_range` logic handles this.
- Name extraction: always `name` field, simpler than Rust.

**Extensions:** `.py`, `.pyi`

**Acceptance criteria:**
- `resolve_tree_path("class::Order::fn::process", Language::Python)` returns correct span.
- `compute_tree_path` produces correct path for top-level functions, class methods, nested classes.
- Roundtrip (compute → resolve → same span) passes for representative Python code.

### M1.3. Go (`lang-go` feature)

**Grammar:** `tree-sitter-go` (0.25.0)

**Kind mappings:**

| Shorthand | Node kind |
|---|---|
| `fn` | `function_declaration` |
| `method` | `method_declaration` |
| `struct` | `type_declaration` → `type_spec` with `struct_type` |
| `interface` | `type_declaration` → `type_spec` with `interface_type` |
| `const` | `const_declaration` |
| `var` | `var_declaration` |

**Design notes:**
- Go methods have receivers and live at top level, not nested inside a struct body. Tree_path encoding: `method::(*MyType).DoThing` or `method::MyType.DoThing`. The method name includes the receiver type for disambiguation.
- `type_declaration` wraps `type_spec` which has the actual name. Name extraction needs to reach into `type_spec` → `name` field.
- No nesting equivalent to Rust's `impl` or Python's class body — all functions/methods are top-level.

**Extensions:** `.go`

**Open design question:** Receiver encoding in tree_path. Options:
1. `method::MyType.DoThing` — simple, matches Go syntax
2. `method::(*MyType).DoThing` — distinguishes pointer/value receivers
3. `struct::MyType::method::DoThing` — uses nested path syntax despite flat AST

Option 1 is recommended — simple and readable, with pointer receiver indicated by `*` prefix when present.

**Acceptance criteria:**
- Functions, methods (pointer + value receiver), struct types, interface types resolve correctly.
- Roundtrip passes for representative Go code.

### M1.4. JavaScript (`lang-javascript` feature)

**Grammar:** `tree-sitter-javascript` (0.25.0)

**Kind mappings:**

| Shorthand | Node kind |
|---|---|
| `fn` | `function_declaration` |
| `class` | `class_declaration` |
| `method` | `method_definition` |
| `const` / `var` / `let` | `variable_declaration` → `variable_declarator` |

**Design notes:**
- Arrow functions assigned to variables (`const foo = () => ...`) are extremely common. These are `variable_declarator` with an `arrow_function` value, not `function_declaration`. The tool tracks them as `fn::foo` when the value is an `arrow_function` or `function` — detecting the pattern in `variable_declarator` and mapping it to the `fn` shorthand.
- Class methods use `method_definition` inside `class_body`. Tree_path: `class::MyClass::method::handleClick`.
- Named vs default exports: export wrappers are transparent — the tool looks through `export_statement` to the inner declaration.

**Extensions:** `.js`, `.mjs`, `.cjs`, `.jsx`

**Acceptance criteria:**
- `function_declaration`, `class_declaration`, `method_definition` all resolve.
- Arrow functions in const declarations map to `fn::name`.
- Export-wrapped declarations resolve correctly.

### M1.5. TypeScript (`lang-typescript` feature)

**Grammar:** `tree-sitter-typescript` (0.23.2) — ships two grammars: `typescript` and `tsx`.

**Additional kind mappings (over JavaScript):**

| Shorthand | Node kind |
|---|---|
| `interface` | `interface_declaration` |
| `type` | `type_alias_declaration` |
| `enum` | `enum_declaration` |

**Design notes:**
- Dual grammar: `.ts`/`.mts`/`.cts` → typescript grammar, `.tsx` → tsx grammar. `detect_language` returns `Language::TypeScript` or `Language::Tsx`.
- Inherits all JavaScript patterns — arrow functions, class methods, export transparency.

**Extensions:** `.ts`, `.tsx`, `.mts`, `.cts`

**Acceptance criteria:**
- All JS tests pass with TS grammar.
- `interface_declaration`, `type_alias_declaration`, `enum_declaration` resolve correctly.
- TSX files parse with tsx grammar.

---

## M2. Deferred languages — design notes

These languages are tracked but not planned for 0.1.x.

### Vue

Vue SFCs are a meta-language: `<template>`, `<script>`, `<style>` blocks containing HTML, JS/TS, and CSS respectively. Supporting tree_path would require:

1. Parse the SFC structure with `tree-sitter-vue`
2. Extract `<script>` content
3. Re-parse with JS/TS grammar
4. Compose a cross-grammar tree_path: `script::fn::setup`

This is a language-in-language extraction pattern not supported by the current single-grammar-per-file architecture. The `tree-sitter-vue` crate (v0.0.3) is also low-maturity.

**Vue users can still use liyi** — `tree_path` stays empty, shift heuristic applies. No degradation of core functionality.

### Markdown

Heading-based tree_path (`heading::Installation::heading::Prerequisites`) is technically feasible and useful for tracking intent on documentation sections. But it's a conceptual extension:

- The item vocabulary (`fn`, `struct`, etc.) doesn't apply.
- Requires a Markdown-specific vocabulary: `heading`, `code_block`, `list_item`.
- The value proposition is different — tracking doc section intent vs code item intent.

Worth a dedicated design note if demand emerges.

---

## M3. Remaining MVP gaps (0.1.x)

**Status:** ✅ Complete — all items implemented and shipped.

These items are from the MVP roadmap's "remaining work" section.

### M3.1. `liyi approve` — interactive review command ✅

The primary mechanism for transitioning intent from "agent-inferred" to "human-approved." Without this, users must hand-edit JSON to set `"reviewed": true`.

- Interactive by default when stdin is a TTY: show intent + source span, prompt `[y]es / [n]o / [e]dit / [s]kip`.
- Batch mode via `--yes` or when non-TTY.
- `--dry-run`, `--item <name>` flags.
- Reanchors on approval (fills `source_hash`, `source_anchor`).

### M3.2. `liyi init` — scaffold command ✅

- `liyi init` — append agent instruction to `AGENTS.md`.
- `liyi init <source-file>` — create skeleton `.liyi.jsonc` sidecar.
- `--force` flag for overwriting existing files.
- `liyi init <source-file> --hints` — populate `_hints` in skeleton sidecar entries with VCS/filesystem signals (commit count, fix-commit count, test presence, docstring lines, file age). Requires git; gracefully degrades (omits VCS hints) when not in a git repo. Opt-in in 0.1.x, may become default later.

### M3.3. Wire remaining diagnostics ✅

1. `RequirementCycle` — cycle detection in pass 2
2. `Untracked` — requirements in source but absent from sidecars
3. `ReqNoRelated` — requirements with no referencing items
4. `MalformedHash` — validate `source_hash` format

### M3.4. Missing golden-file fixtures ✅

1. `req_changed/` — test `ReqChanged` diagnostic
2. `req_cycle/` — test `RequirementCycle` diagnostic (depends on M3.3)

### M3.5. CI setup ✅

GitHub Actions workflow: `cargo test` + `liyi check --root .` on push/PR.

### M3.6. Summary line output ✅

Print count summary after diagnostics: `12 current, 3 stale, 1 unreviewed`.

---

## M4. Git-aware triage (deferred — not planned)

Considered and explicitly rejected for 0.1.x. Recorded here for posterity.

**Proposal:** Store `anchored_at` (git commit hash) per sidecar. Use `git diff <anchored_at>..HEAD` to give the triage agent a bounded, focused diff instead of the full file.

**Why rejected:**
- `source_hash` is already a content-addressed anchor — strictly more robust than a temporal anchor (immune to history rewriting, rebased commits, shallow clones).
- The triage question is "does current code match declared intent?" — answerable from current code + intent alone. History tells you *how* drift happened, not *whether* intent still holds.
- Adds git as a soft dependency. The sidecar model is currently VCS-agnostic.
- Two staleness signals (hash + commit) that can disagree create ambiguity.

**If git context helps triage quality**, it belongs in the triage **workflow** (the agent invokes `git log`/`git blame` at triage time), not the **data layer** (the sidecar schema). Zero schema changes, zero backward-compatibility concerns.

---

## M5. Annotation coverage checks and `--prompt` mode

### M5.1. `MissingRelated` diagnostic ✅

**Status:** Implemented.

Extend the post-pass in `check.rs` to cross-reference `@liyi:related` markers discovered during pass 1 against `"related"` edges in the enclosing item's sidecar spec.

**Implementation:**

1. During pass 1, in addition to collecting `@liyi:requirement` markers, also collect `@liyi:related` markers with their source file, line number, and requirement name.
2. In the post-pass (after pass 2), for each `@liyi:related` marker:
   a. Find the sidecar for the marker's source file.
   b. Find the `itemSpec` whose `source_span` encloses the marker's line number.
   c. If no enclosing item exists, or the enclosing item has no `"related"` key containing the marker's requirement name, emit `MissingRelated`.
3. Add `MissingRelated` variant to `DiagnosticKind` in `diagnostics.rs` with severity `Error`.

**New types:**

```rust
// In diagnostics.rs
enum DiagnosticKind {
    // ...existing variants...
    MissingRelatedEdge { name: String },
}
```

**Message template:** `<item>: ✗ MISSING RELATED — @liyi:related "<name>" in source but no related edge in sidecar`

**Exit code:** 1 (always treated as error).

**Auto-fix:** `--fix` adds the missing edge to the sidecar.

### M5.2. Promote `Untracked` to exit 1 under `--fail-on-untracked` ✅

**Status:** Implemented.

The existing `Untracked` diagnostic (requirements in source but absent from sidecars) currently exits 0. Update it to exit 1 when `--fail-on-untracked` is set (default: true).

**Changes:**
- Add `--fail-on-untracked` / `--no-fail-on-untracked` flag to `cli.rs`.
- Update `compute_exit_code` in `diagnostics.rs` so that `Untracked` respects this flag; `MissingRelatedEdge` remains an unconditional error (exit 1).
- Update existing `untracked` golden fixture expected output if exit code changes.

### M5.3. `--prompt` output mode ⏳

**Status:** Design complete, implementation pending. See `docs/prompt-mode-design.md`.

Add a `--prompt` flag to `liyi check` that emits structured JSON listing every coverage gap with resolution instructions.

**Implementation:**

1. Add `--prompt` flag to the `Check` variant in `cli.rs`.
2. After the check pass, if `--prompt` is active, serialize all `Untracked` and `MissingRelated` diagnostics into the prompt JSON schema (see design doc v8.7).
3. Print to stdout and exit with the appropriate code.
4. `--prompt` is mutually exclusive with `--json` (when `--json` is implemented).

**Output schema:**

```jsonc
{
  "version": "0.1",
  "gaps": [
    {
      "type": "missing_requirement_spec" | "missing_related_edge",
      "requirement": "<name>",
      "source_file": "<repo-relative path>",
      "annotation_line": <line number>,
      "enclosing_item": "<item name>",        // only for missing_related_edge
      "expected_sidecar": "<repo-relative path>",
      "instruction": "<natural-language resolution instruction>"
    }
  ],
  "exit_code": 0 | 1
}
```

**Acceptance criteria:**
- `liyi check --prompt` on a fixture with gaps produces valid JSON matching the schema.
- `liyi check --prompt` on a clean repo produces `{"version": "0.1", "gaps": [], "exit_code": 0}`.
- The JSON includes both `missing_requirement_spec` and `missing_related_edge` gap types.

### M5.4. Golden-file fixtures ✅

**Status:** Partially implemented.

1. ✅ **`missing_related/`**: `@liyi:related` in source, itemSpec exists but lacks the `related` edge. Expected: `MISSING RELATED`.
2. ✅ **`missing_related_pass/`**: Same as above but edge exists. Expected: no diagnostic.
3. ⏳ **`prompt_output/`**: Mixed gaps. Expected: `--prompt` JSON output matches snapshot. (Pending M5.3)

### M5.5. AGENTS.md rule 11 ✅

**Status:** Implemented.

Add rule 11 to the project's own `AGENTS.md`:

> 11\. Before committing, run `liyi check`. If it reports coverage gaps (missing requirement specs, missing related edges), resolve **all** gaps in the same commit. When running in agent mode, consume the `liyi check --prompt` output and apply its instructions. Do not commit with unresolved coverage gaps — CI will reject it.

---

## M6. NL-quoting quine suppression in marker scanner

**Goal:** Enable the scanner to process documentation files (Markdown, READMEs, design docs) without false-positive marker matches on documentary mentions. This unblocks removing `docs/`, `AGENTS.md`, and `README.md` from `.liyiignore`, enabling cross-boundary `@liyi:requirement` / `@liyi:related` edges between design docs and source code.

**Design authority:** Design doc v8.7, *Self-hosting and the quine problem*.

### M6.1. Fenced code block suppression ✅

**Status:** Implemented with unit tests.

Add fenced-block state tracking to `scan_markers` in `markers.rs`.

- Track a `bool` toggled on lines starting with `` ``` `` or `~~~` (after optional leading whitespace).
- When inside a fenced block, skip all marker detection.
- This is the multi-line component — all other checks remain per-line.

### M6.2. Inline backtick span detection ✅

**Status:** Implemented with unit tests.

Before returning a marker match from `find_marker`, check whether the match position falls inside an inline backtick span on the same line.

- Count backtick characters before the match position. Odd count → inside inline code → reject.
- Handles `` `@liyi:module` `` and `` `<!-- @liyi:module -->` `` alike.

### M6.3. Preceding quote character rejection ✅

**Status:** Implemented with unit tests.

If the character immediately before the `@` (or its full-width equivalent after normalization) is a quotation mark, reject the match.

**Rejected characters:** `'` (U+0027), `"` (U+0022), `\`` (U+0060), `\u{2018}` (`'`), `\u{2019}` (`'`), `\u{201C}` (`"`), `\u{201D}` (`"`), `\u{300C}` (`「`), `\u{300D}` (`」`), `\u{00AB}` (`«`), `\u{00BB}` (`»`).

The backtick in this list is redundant with M6.2 but retained as defense-in-depth.

### M6.4. Update `.liyiignore` (~5min)

**Status:** Implemented.

Removed `docs/`, `AGENTS.md`, `README.md`, `README.zh.md` from the project's `.liyiignore`. The NL-quoting checks now handle documentary mentions.

### M6.5. Escape `@liyi:intent` in AGENTS.md JSON schema (~5min)

**Status:** Implemented.

The one remaining unquoted `@liyi:intent` string in AGENTS.md was inside a JSON `"description"` field within a fenced code block (handled by M6.1). Additionally, escaped the `@` as `\u0040` in the JSON string to be consistent with the code-level quine-escape convention.

### M6.6. Golden-file fixtures and unit tests ✅

**Status:** Implemented.

1. Unit tests in `markers.rs` for:
   - Fenced code block suppression (markers inside `` ``` `` blocks not found)
   - Inline backtick suppression (`` `@liyi:module` `` not matched)
   - Preceding-quote suppression (`"@liyi:intent"` not matched)
   - Real markers adjacent to these constructs still matched
2. Golden-file fixture `nl_quoting/` — not created; existing unit tests provide coverage.

### M6.7. Update contributing guides (~15min)

**Status:** Implemented.

Extended the quine-escape sections in both `contributing-guide.en.md` and `contributing-guide.zh.md` to document the NL-quoting convention for documentation files.

**Acceptance criteria:**
- `liyi check` on the project's own repo (with `docs/` no longer ignored) produces no false-positive markers from the design doc.
- The `<!-- @liyi:requirement liyi-check-exit-code -->` block in the design doc is correctly detected as a real marker.
- All existing tests pass.

---

## Priority order (updated)

| Priority | Item | Status | Effort | Unlocks |
|---|---|---|---|---|
| ~~1~~ | ~~M3.1–M3.6 MVP gaps~~ | ✅ Done | — | — |
| ~~2~~ | ~~M5.1 MissingRelated~~ | ✅ Done | — | Annotation coverage |
| ~~3~~ | ~~M5.2 `--fail-on-untracked`~~ | ✅ Done | — | CI-gateable coverage |
| ~~4~~ | ~~M5.4 Golden fixtures~~ | ✅ Done | — | Test coverage for M5 |
| ~~5~~ | ~~M5.5 AGENTS.md rule 11~~ | ✅ Done | — | Convention completeness |
| ~~6~~ | ~~M6.1–M6.3 NL-quoting scanner~~ | ✅ Done | — | Docs processable |
| ~~7~~ | ~~M6.4–M6.5 `.liyiignore` + AGENTS.md~~ | ✅ Done | — | Self-hosting docs |
| ~~8~~ | ~~M6.6 Tests~~ | ✅ Done | — | Regression guard |
| ~~9~~ | ~~M6.7 Contributing guides~~ | ✅ Done | — | Convention documentation |
| 10 | M5.3 `--prompt` output | ⏳ Design | ~3h | Agent-consumable gaps |
| 11 | M1.1 `LanguageConfig` refactor | ⏳ Todo | ~4h | All language support |
| 12 | M1.2 Python | ⏳ Todo | ~2h | Python ecosystem |
| 13 | M1.4 JavaScript | ⏳ Todo | ~2h | JS ecosystem |
| 14 | M1.5 TypeScript | ⏳ Todo | ~1h | TS ecosystem |
| 15 | M1.3 Go | ⏳ Todo | ~3h | Go ecosystem |

---

## AIGC Disclaimer

This document contains content from the following AI agents:

* Claude Opus 4.6

The document is authored by Claude Opus 4.6 with the human designer's input.
