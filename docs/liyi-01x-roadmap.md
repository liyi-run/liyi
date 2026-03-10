# 立意 (Lìyì) — 0.1.x Roadmap

2026-03-06 (updated 2026-03-10)

---

## Overview

This document covers post-MVP work that ships as 0.1.x patch releases. Everything here is additive — no schema changes, no CLI breaking changes, no behavioral regressions.

The MVP roadmap (`docs/liyi-mvp-roadmap.md`) covers the 0.1.0 release. This document picks up where it leaves off.

**Design authority:** `docs/liyi-design.md` v8.8 — see *Structural identity via `tree_path`*, *Multi-language architecture (`LanguageConfig`)*, and *Annotation coverage*.

---

## Current Status Summary

| Milestone | Status | Notes |
|-----------|--------|-------|
| M1 Multi-language tree_path | ✅ Complete | All 5 languages built-in, no feature gates |
| M2 Extended language support | ✅ Complete | C, C++, Java, C#, PHP, ObjC, Kotlin, Swift |
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

**Status:** ✅ Complete — all languages built-in, no feature gates.

**Goal:** Extend tree-sitter-based structural identity from Rust-only to Python, Go, JavaScript, and TypeScript. All grammars are compiled into the binary unconditionally — no Cargo features, no opt-in. The binary-size cost is modest relative to the universality benefit; Python, Go, JavaScript, and TypeScript codebases vastly outnumber Rust codebases, and requiring users to opt in per language would hinder adoption of a tool whose value proposition is universality.

### M1.1. `LanguageConfig` refactor ✅

Extracted language-specific touch points into a data-driven `LanguageConfig` struct:

| Current code | Becomes |
|---|---|
| `KIND_MAP` (hardcoded Rust node kinds) | `LanguageConfig::kind_map` |
| `Language` enum (only `Rust`) | Extended with variants per language |
| `detect_language()` (only `.rs`) | Dispatch table from extensions |
| `make_parser()` (only `tree_sitter_rust`) | `LanguageConfig::ts_language` |
| `node_name()` (`impl_item` special case) | `LanguageConfig::name_overrides` |

The `LanguageConfig` struct (from design doc v8.6):

```rust
struct LanguageConfig {
    ts_language: fn() -> tree_sitter::Language,
    extensions: &'static [&'static str],
    kind_map: &'static [(&'static str, &'static str)],
    name_field: &'static str,
    name_overrides: &'static [(&'static str, &'static str)],
    body_fields: &'static [&'static str],
    custom_name: Option<fn(&Node, &str) -> Option<String>>,
}
```

The `custom_name` callback handles languages with non-trivial name extraction (e.g., Go method receiver encoding, Go `type_declaration` → `type_spec` indirection).

**Acceptance criteria:**
- All existing tests pass with Rust handled via `LanguageConfig` instead of hardcoded paths.
- Adding a new language requires only a new `LanguageConfig` constant — no changes to resolve/compute logic.

### M1.2. Python ✅

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

### M1.3. Go ✅

**Grammar:** `tree-sitter-go` (0.25.0)

**Kind mappings:**

| Shorthand | Node kind |
|---|---|
| `fn` | `function_declaration` |
| `method` | `method_declaration` |
| `type` | `type_declaration` (name extracted from inner `type_spec`) |
| `const` | `const_declaration` (name extracted from inner `const_spec`) |
| `var` | `var_declaration` (name extracted from inner `var_spec`) |

**Design notes:**
- Go methods encode the receiver type in tree_path: `method::(*MyType).DoThing` (pointer receiver) or `method::MyType.DoThing` (value receiver). This disambiguates methods with the same name on different types.
- `type_declaration` wraps `type_spec` which has the actual name. A `custom_name` callback navigates the indirection. A single `type` shorthand covers structs, interfaces, and type aliases — Go type names are unique per package, so no disambiguation is needed.
- No nesting equivalent to Rust's `impl` or Python's class body — all functions/methods are top-level.

**Extensions:** `.go`

**Acceptance criteria:**
- Functions, methods (pointer + value receiver), type declarations (struct + interface), const, var resolve correctly.
- Roundtrip passes for representative Go code.

### M1.4. JavaScript ✅

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

### M1.5. TypeScript ✅

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

## M2. Extended language support

**Status:** ✅ Complete — 8 additional languages built-in, no feature gates.

**Goal:** Extend tree-sitter structural identity to C, C++, Java, C#, PHP, Objective-C, Kotlin, and Swift. All grammars are compiled into the binary unconditionally, matching the M1 design decision. The binary-size cost remains modest (tree-sitter grammars are compact C code) and the universality benefit is significant — C/C++ codebases are where intent drift is most acute and structural anchors most valuable.

### M2.1. C ✅

**Grammar:** `tree-sitter-c` (0.24.1) — the oldest and most mature tree-sitter grammar.

**Kind mappings:**

| Shorthand | Node kind |
|---|---|
| `fn` | `function_definition` |
| `struct` | `struct_specifier` |
| `enum` | `enum_specifier` |
| `typedef` | `type_definition` |

**Design notes:**
- C function names live inside a `declarator` → `function_declarator` → `identifier` chain, not a simple `name` field. A `c_node_name` custom callback recursively unwraps `pointer_declarator`, `parenthesized_declarator`, and `attributed_declarator` wrappers to find the `function_declarator`, then extracts the identifier.
- `type_definition` (typedef) names are in the `declarator` field.
- `.h` files are ambiguous (could be C, C++, or ObjC). Mapped to C by default since C has the simplest grammar and produces valid tree_paths for the overlapping subset.

**Extensions:** `.c`, `.h`

**Acceptance criteria:**
- Functions, structs, enums, typedefs all resolve.
- Roundtrip (compute → resolve → same span) passes.

### M2.2. C++ ✅

**Grammar:** `tree-sitter-cpp` (0.23.4) — second-oldest tree-sitter grammar, extremely mature.

**Kind mappings:**

| Shorthand | Node kind |
|---|---|
| `fn` | `function_definition` |
| `class` | `class_specifier` |
| `struct` | `struct_specifier` |
| `namespace` | `namespace_definition` |
| `enum` | `enum_specifier` |
| `template` | `template_declaration` |
| `typedef` | `type_definition` |
| `using` | `alias_declaration` |

**Design notes:**
- Inherits C's declarator-chain name extraction pattern via a `cpp_node_name` callback.
- `template_declaration` is a transparent wrapper. The callback unwraps it to find the inner declaration (`function_definition`, `class_specifier`, etc.) and extracts the name from there.
- Namespaces use `declaration_list` as their body container; `find_body` finds this via the fallback child search.
- Class methods are `function_definition` inside `field_declaration_list`; the extended `find_body` fallback handles this.
- `enum class` (scoped enums) parse as `enum_specifier` just like plain enums.

**Extensions:** `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hh`, `.hxx`, `.h++`, `.c++`

**Acceptance criteria:**
- Namespaces, classes-in-namespaces, methods-in-classes, standalone functions, enums all resolve.
- Template-wrapped declarations resolve correctly.
- Roundtrip passes through namespace nesting.

### M2.3. Java ✅

**Grammar:** `tree-sitter-java` (0.23.5)

**Kind mappings:**

| Shorthand | Node kind |
|---|---|
| `fn` | `method_declaration` |
| `class` | `class_declaration` |
| `interface` | `interface_declaration` |
| `enum` | `enum_declaration` |
| `constructor` | `constructor_declaration` |
| `record` | `record_declaration` |
| `annotation` | `annotation_type_declaration` |

**Design notes:**
- All node types have a standard `name` field — no custom callback needed.
- Methods are `method_declaration` inside `class_body`. Tree_path: `class::Calculator::fn::add`.
- Records (Java 14+) and annotation types are included for completeness.

**Extensions:** `.java`

**Acceptance criteria:**
- Classes, methods, constructors, interfaces, enums, records all resolve.
- Roundtrip passes for methods nested in classes.

### M2.4. C# ✅

**Grammar:** `tree-sitter-c-sharp` (0.23.1)

**Kind mappings:**

| Shorthand | Node kind |
|---|---|
| `fn` | `method_declaration` |
| `class` | `class_declaration` |
| `interface` | `interface_declaration` |
| `enum` | `enum_declaration` |
| `struct` | `struct_declaration` |
| `namespace` | `namespace_declaration` |
| `constructor` | `constructor_declaration` |
| `property` | `property_declaration` |
| `record` | `record_declaration` |
| `delegate` | `delegate_declaration` |

**Design notes:**
- All node types have a standard `name` field — no custom callback needed.
- Namespaces use `body` field for descent, enabling `namespace::MyApp::class::Foo::fn::Bar` paths.
- Properties are tracked as named items (important for C#'s property-centric design).
- File-scoped namespace declarations (`namespace Foo;`) are not tracked as container items since they have no body to descend into.

**Extensions:** `.cs`

**Acceptance criteria:**
- Namespaces, classes, methods, properties, interfaces, enums, structs all resolve.
- Namespace → class → method nesting roundtrips correctly.

### M2.5. PHP ✅

**Grammar:** `tree-sitter-php` (0.24.2) — uses `LANGUAGE_PHP_ONLY` (pure PHP, no HTML interleaving).

**Kind mappings:**

| Shorthand | Node kind |
|---|---|
| `fn` | `function_definition` |
| `class` | `class_declaration` |
| `method` | `method_declaration` |
| `interface` | `interface_declaration` |
| `enum` | `enum_declaration` |
| `trait` | `trait_declaration` |
| `namespace` | `namespace_definition` |
| `const` | `const_declaration` |

**Design notes:**
- PHP distinguishes `function_definition` (top-level) from `method_declaration` (inside classes). Both have a `name` field.
- `const_declaration` stores its name inside a `const_element` child — a `php_node_name` custom callback handles this.
- Traits are first-class items (important for Laravel/Symfony codebases).
- PHP 8.1 enums are supported.

**Extensions:** `.php`

**Acceptance criteria:**
- Classes, methods, functions, interfaces, traits, enums all resolve.
- Roundtrip passes.

### M2.6. Objective-C ✅

**Grammar:** `tree-sitter-objc` (3.0.2)

**Kind mappings:**

| Shorthand | Node kind |
|---|---|
| `fn` | `function_definition` |
| `class` | `class_interface` |
| `impl` | `class_implementation` |
| `protocol` | `protocol_declaration` |
| `method` | `method_definition` |
| `method_decl` | `method_declaration` |
| `struct` | `struct_specifier` |
| `enum` | `enum_specifier` |
| `typedef` | `type_definition` |

**Design notes:**
- Most ObjC declaration node types lack standard `name` fields. An `objc_node_name` custom callback handles:
  - `function_definition`: C-style declarator chain (shared with C callback).
  - `class_interface` / `class_implementation`: name is a direct child `identifier` or `type_identifier`.
  - `protocol_declaration`: same pattern.
  - `method_declaration` / `method_definition`: ObjC selector names are composed from `keyword_declarator` children (e.g., `initWithFrame:style:`).
- C-level structs and enums use the standard `name` field.
- `class_interface` (`@interface`) and `class_implementation` (`@implementation`) are tracked as separate item types, mirroring ObjC's header/implementation split.

**Extensions:** `.m`, `.mm`

**Acceptance criteria:**
- C functions, structs, and enums resolve (shared with C grammar patterns).
- Roundtrip passes for C-level items.

### M2.7. Kotlin ✅

**Grammar:** `tree-sitter-kotlin-ng` (1.1.0) — the `-ng` fork, compatible with tree-sitter 0.26.x.

**Kind mappings:**

| Shorthand | Node kind |
|---|---|
| `fn` | `function_declaration` |
| `class` | `class_declaration` |
| `object` | `object_declaration` |
| `property` | `property_declaration` |
| `typealias` | `type_alias` |

**Design notes:**
- `class_body` is a positional child of `class_declaration` (not a named field). The `find_body` fallback was extended to search `body_fields` entries as child node kinds, not just field names.
- `property_declaration` names live inside a `variable_declaration` or `simple_identifier` child — handled by `kotlin_node_name` callback.
- `type_alias` names are in a `type_identifier` or `simple_identifier` child.
- `object_declaration` (Kotlin objects / companion objects) has a standard `name` field.
- The original `tree-sitter-kotlin` crate (0.3.x) requires tree-sitter <0.23 and is incompatible. The `-ng` fork from `tree-sitter-grammars` is the maintained successor.

**Extensions:** `.kt`, `.kts`

**Acceptance criteria:**
- Classes, methods-in-classes, objects, functions all resolve.
- Roundtrip passes.

### M2.8. Swift ✅

**Grammar:** `tree-sitter-swift` (0.7.1)

**Kind mappings:**

| Shorthand | Node kind |
|---|---|
| `fn` | `function_declaration` |
| `class` | `class_declaration` |
| `protocol` | `protocol_declaration` |
| `enum` | `enum_entry` |
| `property` | `property_declaration` |
| `init` | `init_declaration` |
| `typealias` | `typealias_declaration` |

**Design notes:**
- All node types have a standard `name` field — no custom callback needed.
- `class_declaration` covers both `class` and `struct` keywords (both use `class_declaration` with a `declaration_kind` field distinguishing them).
- Protocols map naturally to the `protocol` shorthand.
- `init_declaration` is tracked separately from methods since Swift initializers are syntactically distinct.

**Extensions:** `.swift`

**Acceptance criteria:**
- Protocols, classes, methods-in-classes, functions, init all resolve.
- Roundtrip passes.

---

## M2.9. Deferred languages — design notes

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
