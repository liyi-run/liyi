# 立意 (Lìyì) — 0.1.x Roadmap

2026-03-06

---

## Overview

This document covers post-MVP work that ships as 0.1.x patch releases. Everything here is additive — no schema changes, no CLI breaking changes, no behavioral regressions. Users who never enable a Cargo feature or run a new subcommand see zero impact.

The MVP roadmap (`docs/liyi-mvp-roadmap.md`) covers the 0.1.0 release. This document picks up where it leaves off.

**Design authority:** `docs/liyi-design.md` v8.6 — see *Structural identity via `tree_path`* and *Multi-language architecture (`LanguageConfig`)*.

---

## M1. Multi-language `tree_path` support

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

These items are from the MVP roadmap's "remaining work" section — not yet implemented but still 0.1 material.

### M3.1. `liyi approve` — interactive review command

The primary mechanism for transitioning intent from "agent-inferred" to "human-approved." Without this, users must hand-edit JSON to set `"reviewed": true`.

- Interactive by default when stdin is a TTY: show intent + source span, prompt `[y]es / [n]o / [e]dit / [s]kip`.
- Batch mode via `--yes` or when non-TTY.
- `--dry-run`, `--item <name>` flags.
- Reanchors on approval (fills `source_hash`, `source_anchor`).

### M3.2. `liyi init` — scaffold command

- `liyi init` — append agent instruction to `AGENTS.md`.
- `liyi init <source-file>` — create skeleton `.liyi.jsonc` sidecar.
- `--force` flag for overwriting existing files.

### M3.3. Wire remaining diagnostics

1. `RequirementCycle` — cycle detection in pass 2
2. `Untracked` — requirements in source but absent from sidecars
3. `ReqNoRelated` — requirements with no referencing items
4. `MalformedHash` — validate `source_hash` format

### M3.4. Missing golden-file fixtures

1. `req_changed/` — test `ReqChanged` diagnostic
2. `req_cycle/` — test `RequirementCycle` diagnostic (depends on M3.3)

### M3.5. CI setup

GitHub Actions workflow: `cargo test` + `liyi check --root .` on push/PR.

### M3.6. Summary line output

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

## Priority order

| Priority | Item | Effort | Unlocks |
|---|---|---|---|
| 1 | M1.1 `LanguageConfig` refactor | ~4h | All subsequent language support |
| 2 | M3.5 CI setup | ~30min | Automated quality gate |
| 3 | M3.1 `liyi approve` | ~2h | Human review workflow |
| 4 | M3.2 `liyi init` | ~1h | First-run experience |
| 5 | M1.2 Python | ~2h | First non-Rust language |
| 6 | M1.4 JavaScript | ~2h | JS ecosystem |
| 7 | M1.5 TypeScript | ~1h | Incremental over JS |
| 8 | M1.3 Go | ~3h | Go ecosystem (receiver encoding) |
| 9 | M3.3 Wire remaining diagnostics | ~1h | Complete diagnostic coverage |
| 10 | M3.4 Missing fixtures | ~30min | Complete test coverage |
| 11 | M3.6 Summary line | ~20min | UX polish |

---

## AIGC Disclaimer

This document contains content from the following AI agents:

* Claude Opus 4.6

The document is authored by Claude Opus 4.6 with the human designer's input.
