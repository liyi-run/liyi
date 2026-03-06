# 立意 (Lìyì) — Design v8.6

Establish intent before execution · 2026-03-06

---

## The Idea

AI writes your code. You can't read it all. 立意 makes the AI write down what the code is *supposed to do* — in 5 lines you can review and challenge instead of 50.

AI agents write most code. Humans review it but can't read it all. Intent is ephemeral — it lives in prompts, PR descriptions, context windows. When a different agent or human touches the code six months later, the intent is gone. The code is a fact; what it was *meant* to do is a memory.

立意 is the practice of making intent explicit and persistent — written down, reviewable, challengeable, and durable across sessions, context windows, and team turnover:

1. An agent reads existing code and **infers** what it *should* do — not what it *does*. Or: a human (or agent) writes down what code *should* do before it exists — a **requirement**.
2. Either way, intent is persisted in files that survive across sessions, agents, and team changes.
3. A human reviews the intent — 5 lines of inferred spec or a requirement block, not 50 lines of code.
4. Optionally, the reviewer **challenges** the intent — a second model verifies code against intent, or intent against requirement, and reports divergence. On-demand, zero commitment.
5. A CI linter verifies that intents exist, are reviewed, and aren't stale — and that requirements and implementations haven't diverged.
6. Optionally, a second agent reads the reviewed intent and generates adversarial tests designed to break the code — different model, different blind spots.

Intent flows in two directions. **Descriptive**: an agent infers what existing code should do, a human reviews it, and the reviewed spec becomes authoritative. **Prescriptive**: a human (or agent) states a requirement before or alongside coding, and code must satisfy it. Both directions produce the same artifacts, use the same linter, and feed the same challenge and adversarial testing pipeline.

Persistence is the foundation. Review is what persistence enables. Challenge is a trust aid — it lets a reviewer verify intent without reading all the code. Adversarial testing is the payoff — but even without it, persistent reviewed intent is valuable on its own.

The agent instructions are the protocol. The CI linter is what makes it deterministic.

---

## The Name

立意 (lìyì) — "establish intent" — is a concept taught in Chinese elementary writing education (语文课). Before composing an essay, every student learns to 立意: decide the central idea, the purpose, the thesis — before writing a single sentence. 意在笔先: intent before brush.

The concept originates in classical Chinese literary criticism and extends to painting (画论) and calligraphy, where 立意 is the non-negotiable first step of any creative work. The artist decides what the piece is *meant to invoke* before executing it.

English has analogues — "thesis statement," "controlling idea," "premise" — but none that name the act of *deciding intent as a prerequisite step*. We transliterate rather than translate because no English term carries the same connotation: intent first, everything else follows.

In the classroom, 立意 is not complete at declaration — the teacher challenges it: 太浅了 (too shallow), 偏题了 (off-topic), 太俗了 (too cliché). The student refines before writing. Challenge is part of the practice, not an addition to it. The software convention preserves both steps: establish intent, then challenge it.

It occurred to the designer that 立意, a practice every Chinese student learns before age ten, names exactly the gap described above: intent is ephemeral in AI-assisted development, and the missing step is establishing it before execution. The name is recognition, not metaphor — the practice already existed; the software convention formalizes it.

---

## File Layout

Two levels of intent, two formats:

```
src/billing/
├── README.md                  ← has a 立意 section
├── money.rs                   ← source
├── money.rs.liyi.jsonc        ← item-level intents (JSONC, machine-friendly)
├── orders.rs
├── orders.rs.liyi.jsonc
└── .liyiignore                 ← file-level exclusions (gitignore syntax)
```

### `.liyi/` directory

Tool-generated artifacts live in a `.liyi/` directory at the repository root:

```
.liyi/
├── triage.json                ← latest triage report (agent-produced, liyi-validated)
└── (future: cache files, intermediate state)
```

`.liyi/` is `.gitignore`'d by default — the triage report is derived from sidecars + source + LLM, not a source of truth. Teams that want audit trails can commit it or archive it as a CI build artifact. This follows the same pattern as `.mypy_cache/`, `.ruff_cache/`, or `.pytest_cache/` — a project-level workspace for tool-generated artifacts that shouldn't pollute the source tree.

The `.liyi/` directory is distinct from `.liyi.jsonc` sidecar files (which are co-located with source, committed, and are source of truth). Sidecars are durable; `.liyi/` contents are ephemeral.

### Scope: per-repository

立意 operates within one repository. The linter walks one tree; the agent reads one codebase; specs reference paths within that repo.

This is a deliberate scope constraint with known tradeoffs. The linter's staleness model depends on co-located source files and line-number spans; crossing repo boundaries would require the linter to access source files outside its checkout, something neither CI environments nor agent sandboxes provide by default. Conway's law (software structure mirrors communication structure) explains why per-repo scope often works: repo boundaries tend to reflect organizational boundaries, and intent is local knowledge.

- **Monorepo**: intent flows freely. An agent working in `src/billing/` can read `src/auth/`’s `@liyi:module` block. The linter walks the whole tree. Module-level invariants can span directories. The intent dependency graph (future) resolves to local paths.
- **Polyrepo**: intent stops at the repo boundary. Each repo specs its own code — including its assumptions about dependencies (“I call `verify_token` and expect it to return `Claims` or error”). The consuming repo’s intent describes *how it uses* the dependency, not *what the dependency does internally*.

Cross-repo intent *sharing* can be useful context — an agent reading a dependency’s shipped `.liyi.jsonc` from a package tree gathers context to write its own specs — but it’s not a transitive obligation. The intent informs; it doesn’t cascade. No mechanism for cross-repo intent enforcement is planned.

**Why not a centralized "spec repo"?** It's tempting to keep all intents in one repository — a single place to review intent across the organization, decoupled from implementation repos. This doesn't work with 立意, and the reasons are structural:

- **Staleness requires co-location.** `source_span` and `source_hash` reference lines in the source file. If the spec lives in a different repo, the linter can't hash the source. Cross-repo file access is a security boundary, not a missing feature.
- **PR review flow requires co-location.** When a developer changes `money.rs`, the co-located `money.rs.liyi.jsonc` diff appears in the same PR. Separating specs into another repo means code changes silently leave specs behind — staleness becomes the norm, not the exception.
- **It fights Conway's law.** A centralized spec repo implies centralized authority over intent. But intent is local knowledge: the agent or developer working on the code understands what the function should do. Separating intent from code re-creates the communication gap Conway's law says produces bad software.

System-level architectural constraints ("all inter-service communication uses mTLS," "no service charges a customer twice for the same order") are legitimately centralized — but those are architecture documents and ADRs, not item-level specs. They don't need `source_span` or `source_hash`; they're prose. 立意's item-level convention stays co-located, per repo.

The gap is genuine: in a polyrepo setup, no mechanism deterministically verifies that `service-a`'s assumptions about `auth-lib` match `auth-lib`'s actual behavior. Repo-level and module-level intent prose (`@liyi:module`) can *state* these cross-boundary assumptions ("this service expects `auth-lib` to return `Claims` or error, never panic"), and adversarial tests can *exercise* them — but the linter can't check them against the upstream source. This is an honest limitation of per-repo scope, bridged by prose and testing, not by tooling. Closing it deterministically would require workspace-scoped SCM — a system where cross-repo content dependencies are a native primitive — which no publicly available SCM provides today. Complementary tools (contract testing, API schema validation, integration tests) address parts of the problem from a different angle; 立意 doesn't replace them and isn't replaced by them.

### Module-level: `@liyi:module` marker

Module-level intent is prose describing cross-function invariants. It can live anywhere — Markdown files, source-level doc comments, wherever the team already writes module documentation. The `@liyi:module` marker is the universal signal the linter keys on.

#### In a `README.md` (or any Markdown file)

```markdown
# Billing

This module handles monetary operations.

## 立意
<!-- @liyi:module -->

All monetary amounts carry their currency. No function in this module
silently converts between currencies — mismatches must be explicit errors.
Precision must never be lost through rounding without an explicit
rounding parameter.

## Usage
...
```

The heading makes the section discoverable in the rendered page and GitHub's outline sidebar. The `<!-- @liyi:module -->` comment marks the block for the linter. Both are present: the heading is for humans and agents, the marker is for machines.

Preferred heading text, in order:

1. **`立意`** — the project's own term. Use it if your team can input hanzi and finds it acceptable.
2. **`Liyi`** — if you agree with the Chinese framing but lack a Chinese IME or hanzi support.
3. **`Intent`** (or your own language) — the heading is for humans; use whatever word your team understands.

The linter never inspects the heading. It only matches `@liyi:module`.

**Convention: use the doc markup language's comment syntax for the marker.** Most doc rendering pipelines go through a markup language — Markdown, reStructuredText, etc. — and each has a native comment syntax that is invisible in rendered output. Use that syntax so the marker never leaks into documentation.

The linter matches the literal string `@liyi:module` — it doesn't care about surrounding syntax.

#### In a dedicated `LIYI.md`

```markdown
# 立意
<!-- @liyi:module -->

Currency operations for the billing system.

All monetary amounts carry their currency. No function in this module
silently converts between currencies — mismatches must be explicit errors.
Precision must never be lost through rounding without an explicit
rounding parameter.
```

A heading is still required (most Markdown linters enforce it).

#### In source code (when the host language has module-level doc conventions)

Doc comments are rendered through a markup language. Use that markup's comment syntax for the marker, just as we use HTML comments in Markdown files.

Rust — top of `mod.rs` or `lib.rs` (rustdoc renders Markdown):

```rust
//! Currency operations for the billing system.
//!
//! <!-- @liyi:module -->
//! All monetary amounts carry their currency. No function
//! in this module silently converts between currencies — mismatches must
//! be explicit errors. Precision must never be lost through rounding
//! without an explicit rounding parameter.
```

Python/Sphinx — module docstring (Sphinx renders reStructuredText):

```python
"""Currency operations for the billing system.

.. @liyi:module
   All monetary amounts carry their currency. No function
   in this module silently converts between currencies — mismatches must
   be explicit errors. Precision must never be lost through rounding
   without an explicit rounding parameter.
"""
```

`.. @liyi:module` is a reST comment — invisible in Sphinx output. The indented body is the intent; it ends at the first un-indented line (reST's own block structure).

Python/mkdocstrings — module docstring (mkdocstrings renders Markdown):

```python
"""Currency operations for the billing system.

<!-- @liyi:module -->
All monetary amounts carry their currency. No function
in this module silently converts between currencies — mismatches must
be explicit errors. Precision must never be lost through rounding
without an explicit rounding parameter.
"""
```

Go — `doc.go` (godoc is plain text — no markup comment syntax available):

```go
// Package billing handles currency operations.
//
// # 立意 @liyi:module
//
// All monetary amounts carry their currency. No function
// in this package silently converts between currencies — mismatches must
// be explicit errors. Precision must never be lost through rounding
// without an explicit rounding parameter.
package billing
```

Go 1.19+ doc comments support `#` headings. Since godoc has no markup comment syntax, the marker is visible in rendered output. Embedding it in the heading (`# 立意 @liyi:module`) gives it structure rather than leaving it as a stray annotation.

#### Convention summary

| Location | Markup | Marker syntax |
|---|---|---|
| `.md` files (README, LIYI.md, etc.) | Markdown | `<!-- @liyi:module -->` — has a 立意 section |
| `mod.rs`, `lib.rs` (rustdoc) | Markdown | `//! <!-- @liyi:module -->` |
| Python docstring (Sphinx) | reST | `.. @liyi:module` |
| Python docstring (mkdocstrings) | Markdown | `<!-- @liyi:module -->` |
| `doc.go` (godoc) | plain text | `// # 立意 @liyi:module` (visible, in heading) |

The `@liyi:module` string is the only thing the linter looks for. Everything else — heading style, file choice, comment syntax — is team preference.

The linter only checks for the *presence* of `@liyi:module` in a directory's files. It does not parse or consume the intent prose — that text is for humans, agents, and code review; the linter just confirms it exists. Post-MVP, a closing `/@liyi:module` tag may be supported for mechanical extraction of module intent prose from long files; the 0.1 linter does not look for it.

- Optional. Not every directory needs one. The agent infers it when cross-function invariants are apparent.
- The linter can report directories that have `.liyi.jsonc` files but no `@liyi:module` marker (informational, not a failure by default).

### Item-level: `.liyi.jsonc`

"Item" rather than "function" — because `source_span` can point to a function, a struct with derive attributes, a macro invocation, a decorated endpoint, or any other intent site. The term follows Rust specification prior art, where "item" is anything that can appear at module level.

**Naming convention.** The sidecar filename is the source filename with `.liyi.jsonc` appended: `money.rs` → `money.rs.liyi.jsonc`. Always append to the full filename, never strip the extension. This avoids ambiguity when files share a stem but differ in extension (`money.rs` and `money.py` would otherwise both claim `money.liyi.jsonc`). The rule is mechanical: one source file, one sidecar, derivable by concatenation.

One per source file, co-located:

The `source` path is relative to the repository root — the same path you'd pass to `git show`. The JSONC header comment is informational (the linter ignores it).

```jsonc
// Generated by 立意 protocol — agent: claude-opus-4, 2026-03-05
{
  "version": "0.1",
  "source": "src/billing/money.rs",
  "specs": [
    {
      "item": "add_money",
      "intent": "Add two monetary amounts of the same currency. Must be commutative. Must reject mismatched currencies with an error, not a panic. Must not overflow silently.",
      "source_span": [42, 58],
      "tree_path": "fn::add_money",
      "confidence": 0.94
    },
    {
      "item": "convert_currency",
      "intent": "=doc",
      "source_span": [60, 85],
      "tree_path": "fn::convert_currency",
      "confidence": 0.87
    }
  ]
}
```

`source_hash`, `source_anchor`, and `tree_path` are tool-managed — the agent writes only `source_span` and the tool fills in the rest (see *Per-item staleness* and *Structural identity via `tree_path`* below). Agents MAY write `tree_path` if they can infer the AST path, but the tool will overwrite it with the canonical form on the next `liyi reanchor`. `"intent": "=doc"` is a reserved sentinel meaning "the docstring already captures intent" — the agent uses it when the source docstring contains behavioral requirements (constraints, error conditions, properties), not just a functional summary (see *`"=doc"` in the sidecar* below).

`"version"` is required. The linter checks it and rejects unknown versions. This costs nothing now and prevents painful migration when the schema evolves (e.g., adding `"related"` edges, structured fields in post-0.1). A JSON Schema definition ships alongside the linter for editor validation and autocompletion (see *Appendix: JSON Schema* below). When the schema changes, the linter will accept both `"0.1"` and the new version during a transition window, and `liyi reanchor --migrate` will upgrade sidecar files in place.

**`liyi reanchor --migrate` behavior.** When the schema version changes (e.g., 0.1 → 0.2), `--migrate` reads each `.liyi.jsonc`, adds any newly required fields with default values, removes deprecated fields, updates `"version"` to the new version, and writes the file back. It is idempotent — running it twice produces the same output. It does not re-hash spans or re-infer intent; it only transforms the schema envelope. Migration is always additive in 0.x: no field present in 0.1 will change meaning, only new fields may appear.

After human review — either the human adds `@liyi:intent` in the source file (see *Source-level intent* below), or sets `"reviewed": true` in the sidecar via CLI or IDE code action. Both paths mark the item as reviewed. When `"reviewed"` is set to `true`, `"confidence"` is removed — a human voucher replaces agent self-assessment. If the source later changes and the agent re-infers (producing a new unreviewed spec), `"confidence"` reappears:

```jsonc
{
  "version": "0.1",
  "source": "src/billing/money.rs",
  "specs": [
    {
      "item": "add_money",
      "reviewed": true,
      "intent": "Add two monetary amounts of the same currency. Must be commutative. Must reject mismatched currencies with an error, not a panic. Must not overflow silently.",
      "source_span": [42, 58],
      "tree_path": "fn::add_money",
      "source_hash": "sha256:a1b2c3...",
      "source_anchor": "pub fn add_money(a: Money, b: Money) -> Result<Money, CurrencyError> {"
    }
  ]
}
```

`"reviewed"` defaults to `false` when absent. The linter considers an item reviewed if **either** `"reviewed": true` in the sidecar **or** `@liyi:intent` exists in source. Source intent takes precedence for adversarial testing — it's the human's assertion, not the agent's inference. See *Source-level intent* and *Security model* below.

### Why a single `intent` field, not structured pre/postconditions?

The testing agent reads NL. It doesn't need `preconditions`, `postconditions`, `properties`, and `examples` as separate structured arrays. A single `intent` string:

- Is faster for the human to review (one paragraph, not five fields).
- Is easier for the agent to generate (free-form NL, not a bespoke schema).
- Is sufficient for adversarial test generation (the testing agent extracts what it needs from prose).
- Can contain structured information if the author wants ("Must be commutative" is a property; "Rate must be positive" is a precondition) — but doesn't mandate it.

Structured fields can come later (0.2+) if tooling demands them.

### Why JSONC for item specs, not Markdown?

The linter needs to reliably read `source_span`, `source_hash`, and `related` edges. These are machine-oriented fields. JSONC gives `json["specs"][0]["source_span"]` — one line of code. Markdown with HTML comments requires walking an AST and associating metadata comments with headings — fragile and 40+ extra lines of parsing.

Module-level intent is pure prose → Markdown wins.
Item-level intent carries machine metadata → JSONC wins.

### Per-item staleness via `source_span`

`source_span` is a closed interval of 1-indexed line numbers: `[42, 58]` means lines 42 through 58, inclusive. This matches editor line numbers, `git blame` output, and coincidentally the mathematical convention for closed intervals. `source_hash` is always `sha256:<hex>` — the SHA-256 digest of those lines after normalizing line endings to `\n` (LF). This ensures cross-platform consistency: a Windows developer with `core.autocrlf=true` and a Linux CI runner produce identical hashes for identical content. No other hash algorithm is supported in 0.1. `source_anchor` is the literal text of the first line of the span — used by the linter for efficient shift detection (see below).

Both `source_hash` and `source_anchor` are **tool-managed fields**. The agent writes only `source_span` — the tool (`liyi reanchor`, or `liyi check --fix`) computes the hash and anchor deterministically from the source file. This is the same principle as not letting agents author lockfile checksums: the tool reads the actual bytes, so fabricated or hallucinated hashes are impossible.

The agent records each item's line range (`source_span`) when writing the spec. The linter reads those lines from the source file, hashes them, and compares against `source_hash`. This gives per-item staleness without the linter needing to parse any language — it just reads a slice of lines.

**Blast radius of line-number shifts.** Any edit that changes line numbers — inserting, deleting, or splitting lines — invalidates every spec whose `source_span` falls at or below the edit point. The span now points to different lines, the hash mismatches, and the linter flags them all stale. Add an import at line 3 in a file with 20 specs, and all 20 go stale. This is the real cost of line-number-based spans.

The correct mitigation is language-aware span anchoring — resolving spec positions by AST node identity (e.g., "the function named `add_money` in this file") rather than line numbers. This is what `tree_path` provides (see *Structural identity via `tree_path`* below).

Without a `tree_path`, the fallback is: batch false positives on any line-shifting edit, corrected on the next agent inference pass. The damage is transient and mechanical — the agent re-reads the file, re-records spans, re-hashes — but noisy in CI until it does. Still fewer false positives than file-level hashing (where a docstring typo marks every spec in the file stale with no way to distinguish which items actually changed).

**Span-shift detection (included in 0.1).** When the linter detects a hash mismatch and no `tree_path` is available (or tree-sitter has no grammar for the language), it falls back to scanning ±100 lines for content matching the recorded hash. If the same content appears at an offset (e.g., shifted down by 3 lines because an import was added), the linter reports `SHIFTED` rather than `STALE`. With `--fix`, the span is auto-corrected in the sidecar; without `--fix`, the linter reports the shift but does not write. Once a delta is established for one item, subsequent items in the same file are adjusted by the same delta before checking — so a single import insertion resolves in one probe, not twenty. If no match is found within the window, the linter gives up and reports `STALE` as usual. This is the same heuristic `patch(1)` uses with a fuzz factor — a linear scan over a bounded window, ~50 lines, no parser. Combined with `liyi reanchor`, this eliminates the most common source of false positives (line-shifting edits) without language-specific tooling. For files with `tree_path` populated, tree-sitter-based anchoring supersedes this heuristic entirely — see the next section.

### Structural identity via `tree_path`

`tree_path` is an optional field on both `itemSpec` and `requirementSpec` that provides **structural identity** — matching a spec to its source item by AST node path rather than line number. When present and non-empty, `liyi reanchor` and `liyi check --fix` use tree-sitter to locate the item by its structural position in the parse tree, then update `source_span` to the item's current line range. This makes span recovery deterministic across formatting changes, import additions, line reflows, and any other edit that moves lines without changing the item's identity.

**Format.** A `tree_path` is a `::` delimited path of tree-sitter node kinds and name tokens that uniquely identifies an item within a file. Examples:

| Source item | `tree_path` |
|---|---|
| `pub fn add_money(…)` | `fn::add_money` |
| `impl Money { fn new(…) }` | `impl::Money::fn::new` |
| `struct Money { … }` | `struct::Money` |
| `mod billing { fn charge(…) }` | `mod::billing::fn::charge` |
| `#[test] fn test_add()` | `fn::test_add` |

The path identifies the item by node kind and name, not by position. The tool constructs the path by walking the tree-sitter CST from root to the node that covers `source_span`, recording each named ancestor. This is deterministic — the same source item always produces the same path regardless of where it appears in the file.

**Behavior during reanchor and check.**

1. `liyi reanchor`: Parse the source file with tree-sitter. For each spec with a non-empty `tree_path`, query the parse tree for a node matching the path. If found, update `source_span` to the node's line range, recompute `source_hash` and `source_anchor`. If not found (item was renamed or deleted), report an error — do not silently fall back.
2. `liyi check --fix`: Same tree-sitter lookup. If the hash mismatches but the `tree_path` resolves to a valid node, update the span (the item moved but is still present). If the `tree_path` doesn't resolve, fall back to span-shift heuristic.
3. `liyi check` (without `--fix`): Use `tree_path` to verify the span points to the correct item. If it doesn't (span drifted, but `tree_path` still resolves), report `SHIFTED` with the correct target position.

**Diagnostic clarity.** When a spec has no `tree_path` and the shift heuristic also fails, the diagnostic indicates why tree-path recovery was skipped — e.g., "no tree_path set, falling back to shift heuristic" — so that users can add the missing field or run `liyi reanchor` to auto-populate it. Diagnostics distinguish "no tree_path available" from "tree_path resolution failed (item may have been renamed or deleted)."

**Empty string fallback.** When `tree_path` is `""` (empty string) or absent, the tool falls back to the current line-number-based behavior — span-shift heuristic, `source_anchor` matching, delta propagation. This accommodates:

- **Macro invocations** where the interesting item is the macro call, not a named AST node.
- **Generated code** where tree-sitter may not produce useful node kinds.
- **Complex or contrived cases** where the agent or human determines that a tree path is non-obvious or ambiguous.

The agent MAY set `tree_path` to `""` explicitly to signal "I considered structural identity and it doesn't apply here." Absence of the field is equivalent to `""`. `liyi reanchor` auto-populates `tree_path` for every spec where a clear structural path can be resolved from the current `source_span` and a supported tree-sitter grammar — agents need not set it manually. When the span doesn't correspond to a recognizable AST item (macros, generated code, unsupported languages), the tool leaves `tree_path` empty.

**Language support.** Tree-sitter support is grammar-dependent. In 0.1, Rust is the primary supported language (via `tree-sitter-rust`). For unsupported languages, `tree_path` is left empty and the tool falls back to line-number behavior. Adding a language is a matter of adding its tree-sitter grammar crate and a small mapping of node kinds — no changes to the core protocol or schema.

**Multi-language architecture (`LanguageConfig`).** The `tree_path` implementation is designed to be language-extensible via a data-driven configuration per language. Each supported language provides:

| Config field | Purpose | Example (Rust) | Example (Python) |
|---|---|---|---|
| `ts_language` | Tree-sitter grammar reference | `tree_sitter_rust::LANGUAGE` | `tree_sitter_python::LANGUAGE` |
| `extensions` | File extensions that select this language | `["rs"]` | `["py", "pyi"]` |
| `kind_map` | Shorthand → tree-sitter node kind | `fn → function_item` | `fn → function_definition` |
| `name_field` | Field name for extracting the item name | `"name"` | `"name"` |
| `name_overrides` | Per-kind overrides for name extraction | `impl_item → "type"` | — |
| `body_fields` | Field names for nested item containers | `["body", "declaration_list"]` | `["body"]` |

The shorthand vocabulary (`fn`, `struct`, `class`, `mod`, `impl`, `trait`, `enum`, `const`, `static`, `type`, `macro`, `interface`, `method`) is shared across languages — `fn` always means "function-like item" regardless of whether the underlying node kind is `function_item` (Rust), `function_definition` (Python/Go), or `function_declaration` (JS/TS). The `tree_path` format remains the same: `fn::add_money`, `class::Order::fn::process`.

Each language is gated behind a Cargo feature (`lang-python`, `lang-go`, `lang-javascript`, `lang-typescript`) so users only pay binary-size cost for languages they need. A `lang-all` convenience feature includes everything.

**Planned languages (0.1.x):**

| Language | Grammar crate | Notes |
|---|---|---|
| Python | `tree-sitter-python` | Flat AST; methods are `function_definition` inside `class_definition` body. No `impl`-block equivalent. |
| Go | `tree-sitter-go` | `type_declaration` → `type_spec` indirection for structs/interfaces. Methods have receivers and live at top level — tree_path encodes as `method::(*MyType).DoThing` or `fn::DoThing`. |
| JavaScript | `tree-sitter-javascript` | Arrow functions in `const` declarations are pervasive — `const foo = () => ...` maps to `fn::foo` (tracking the `variable_declarator` when its value is an `arrow_function`). |
| TypeScript | `tree-sitter-typescript` | Superset of JS; adds `interface_declaration`, `type_alias_declaration`, `enum_declaration`. Dual grammar: `.ts` → typescript, `.tsx` → tsx. |

**Deferred languages:**

| Language | Reason |
|---|---|
| Vue | SFCs are a meta-language with embedded JS/TS inside `<script>` blocks. Requires language-in-language extraction not supported by the current single-grammar-per-file architecture. `tree-sitter-vue` (v0.0.3) is low-maturity. Vue users can still use liyi — `tree_path` is empty, shift heuristic applies. |
| Markdown | Heading-based tree_path (`heading::Installation::heading::Prerequisites`) is technically feasible and useful for tracking intent on documentation sections. But it's a conceptual extension — the item vocabulary (`fn`, `struct`) doesn't apply, requiring a Markdown-specific vocabulary (`heading`, `code_block`). Deferred as a distinct design note. |

### Edge cases

The linter handles malformed or outdated specs defensively. Every case below produces a clear diagnostic; none silently passes.

- **`source_span` past EOF.** The source file was truncated or shortened below the span's range. Report as stale — the source changed. Message: `source_span [42, 58] extends past end of file (37 lines)`.
- **Source file deleted or renamed.** The `"source"` path no longer exists. Report as error — the spec is orphaned. The `.liyi.jsonc` should be deleted or renamed to match. Fails `--fail-on-stale`. (The linter could detect and suggest renames via content matching post-MVP; for 0.1, it only reports the orphan.)
- **Inverted or zero-length `source_span`.** `start > end` or `start == end`. Report as error — `invalid source_span [58, 42]` or `empty source_span [42, 42]`. A zero-length span hashes nothing, which is never a useful staleness check.
- **Malformed `source_hash`.** Doesn't match the expected format (`sha256:<hex>`). Report as error — `malformed source_hash`. This is a data integrity issue, distinct from staleness.
- **Overlapping `source_span` ranges.** Two specs in the same file claim overlapping lines. Allowed — an `impl` block and one of its methods may legitimately overlap, or a macro invocation may be the intent site for multiple logical items. The linter hashes each span independently.
- **Duplicate `"item"` values.** Two specs with the same `"item"` string in one `.liyi.jsonc` — e.g., two `impl` blocks both containing a method called `new`. Allowed. `"item"` is a display name for humans, not a unique key. Identity is the composite of `"item"` + `"source_span"`. If both name and span are identical, the linter warns (likely a duplicate entry).

### Macros, metaprogramming, and generated code

Not all items exist literally in source. Macros, decorators, derive attributes, metaclasses, and code generators can create items that have no visible `fn`/`def`/`func` in the source tree.

The convention handles this in three layers:

**Generated output files** (protobuf bindings, OpenAPI clients, GraphQL types) — already covered by `.liyiignore`. Intent belongs to the schema or definition file, not the generated output. Spec the `.proto`, not `*.pb.go`.

**Macros and decorators that create or transform behavior** (Rust `#[derive(Serialize)]`, Python `@app.route`, C preprocessor macros) — the spec attaches to the *intent site*: the line(s) in source where the human decided "this should exist." `source_span` doesn’t require a `fn` keyword — it’s just a line range. The agent picks whatever lines represent the logical source of the behavior:

```rust
// Intent site: the struct + derive + field attributes
#[derive(Serialize, Deserialize)]
struct Order {
    id: u64,
    #[serde(skip)]
    internal_cache: HashMap<String, Value>,
}
```

```python
# Intent site: the decorated function definition
@app.route("/users/<id>")
@require_auth(role="admin")
def get_user(id: str): ...
```

The agent understands macros semantically — it knows `#[derive(Serialize)]` generates serialization, knows what invariants matter. This is a strength of the agent-centric design: no static tool can do this across languages, but the agent handles it naturally.

**Long-range dependencies.** `source_span` is a *local* heuristic. It catches changes to the item itself. It does not catch action at a distance — a change to a decorator’s definition, a schema file, a base class, or a trait impl that alters the behavior of the specced code without touching its source lines.

This is an inherent limitation of per-item staleness (tests have it too — a passing test doesn’t mean its transitive dependencies haven’t changed semantics). The convention addresses it at two other layers:

- **Module-level intent** (`@liyi:module`) captures cross-cutting invariants (“all serialization must round-trip cleanly,” “no endpoint is accessible without authentication”). These invariants remain valid and testable regardless of where the change happened.
- **Adversarial testing** is the designed safety net for semantic drift. A test generated from “must reject unauthenticated requests” will catch a change to `require_auth`’s behavior even though the specced function’s `source_hash` didn’t change.

**Future direction: code-level dependency graph.** Beyond requirement edges, specs could optionally declare code-level dependencies: `"depends_on": ["src/auth/middleware.rs:require_auth"]`. If any dependency's hash changes, the dependent spec is also flagged stale. The agent is the natural thing to populate this (it already understands call graphs); the linter just follows the edges. This is not in scope for 0.1 — the combination of local staleness + requirement tracking + module invariants + adversarial testing covers most real cases — but it's the shape of a tighter answer for teams with highly interconnected code.

---

## Requirements and Dependency Edges

Intent flows in two directions. The default 立意 workflow is **descriptive**: an agent reads code and infers what it should do. But intent can also be **prescriptive**: a human (or agent) states a requirement, and code must satisfy it. Both produce tracked artifacts; the linter handles both.

### `@立意:需求` / `@liyi:requirement` — named requirements

A requirement is a named, freeform prose block that lives anywhere the linter walks — source comments, Markdown files, doc comments. The `@liyi:requirement <name>` marker declares it:

```python
# @立意:需求（多币种加法 考虑舍入）
# 同币种的两笔金额相加。不同币种必须抛异常，不得静默失败。
# 加法必须满足交换律。不得静默溢出。舍入规则由币种决定。
```

```markdown
<!-- @liyi:requirement multi-currency-addition -->
Add two monetary amounts of the same currency. Must reject mismatched
currencies with an error, not a panic. Must be commutative. Must not
overflow silently.
```

```python
# @立意:需求 인출한도
# 1일 인출 한도를 초과하면 거래를 거부한다.
```

The requirement text is the tracked artifact — the comment block itself is the intent site. The linter discovers `@liyi:requirement` markers during its file walk, records each one's location and hash in `.liyi.jsonc` as a tracked entry:

```jsonc
{
  "version": "0.1",
  "source": "src/billing/README.md",
  "specs": [
    {
      "requirement": "multi-currency-addition",
      "source_span": [5, 9],
      "source_hash": "sha256:...",
      "source_anchor": "<!-- @liyi:requirement multi-currency-addition -->"
    }
  ]
}
```

No `intent` field — the requirement text lives at the source site, not duplicated in the sidecar. No `reviewed` — the act of writing a requirement *is* the assertion of intent; provenance belongs to VCS (`git blame` tells you who wrote it and when). The `"requirement"` key itself signals prescriptiveness — no separate boolean needed.

**Name syntax.** If the first non-whitespace character after the keyword is `(` or `（`, the name is everything inside the matching `)` / `）`. Otherwise, the name is the first whitespace-delimited token. This means simple single-token names need no delimiters (`@liyi:requirement auth-check`), while names with internal spaces use parens (`@立意:需求（多币种加法 考虑舍入）`). See *Marker normalization* below for how the linter handles half-width / full-width equivalence.

**Naming and scope.** Requirement names are unique per repository. The linter reports an error if two `@liyi:requirement` markers declare the same name. Names are matched as exact strings (case-sensitive) after trimming leading/trailing whitespace inside parens. The name is a human-readable identifier, not a path — it can be in any language. No character set restriction: `multi-currency-addition`, `多币种加法`, and `인출한도` are all valid names.

**Requirements can live anywhere:** in the source file near the code they govern, in `README.md` alongside `@liyi:module`, in a dedicated requirements file, or in doc comments. The linter scans all non-ignored files for the marker.

**End-of-block markers.** The linter does not require an explicit end marker for requirement blocks — `source_span` in the sidecar defines the block boundaries. An optional `@liyi:end-requirement` (or `@立意:需求止`) marker is **not supported in 0.1** — the linter does not look for it. A future version could accept it for visual clarity in Markdown files where contiguous-comment heuristics don't apply; adding it would be additive and non-breaking.

### `@立意:有关` / `@liyi:related` — dependency edges

The `@liyi:related <name>` annotation declares that a code item participates in a named requirement. The same name syntax applies — parentheses for names with spaces:

```python
# @立意:有关（多币种加法 考虑舍入）
def add_money(a: Money, b: Money) -> Money: ...
```

```rust
// @liyi:related multi-currency-addition
fn add_money(a: Money, b: Money) -> Result<Money, CurrencyError> { ... }
```

The agent infers descriptive intent for the item as usual. The annotation creates an explicit dependency edge in the `.liyi.jsonc`. The agent writes `source_span`, `intent`, and the `related` mapping (names only — the tool fills in hashes):

```jsonc
// Agent writes:
{
  "item": "add_money",
  "intent": "Add two monetary amounts of the same currency...",
  "related": { "multi-currency-addition": null },
  "source_span": [42, 58]
}

// After tool fills in hashes and anchor:
{
  "item": "add_money",
  "intent": "Add two monetary amounts of the same currency...",
  "related": {
    "multi-currency-addition": "sha256:..."
  },
  "source_span": [42, 58],
  "source_hash": "sha256:a1b2c3...",
  "source_anchor": "pub fn add_money(a: Money, b: Money) -> Result<Money, CurrencyError> {"
}
```

`"related"` is an object mapping requirement names to the requirement's `source_hash` at the time the spec was last written or reviewed. The agent writes names with `null` values; the tool fills in hashes. The linter compares each recorded hash against the requirement's current hash — if they differ, the requirement changed since this spec was last reviewed. An item can participate in multiple requirements.

The agent can infer `@liyi:related` annotations (it understands which functions relate to which business requirements), or the human can write them. Either way, the linter resolves each name to a tracked requirement entry and compares hashes.

### Transitive staleness

The linter checks two kinds of staleness independently:

| Condition | Status | Meaning |
|---|---|---|
| Item hash mismatch | **STALE** | Code changed. Agent re-infers descriptive intent. |
| Item hash found at different offset | **SHIFTED** | Lines moved but content unchanged. Span auto-corrected. |
| Referenced requirement hash mismatch | **REQ CHANGED** | Requirement changed. Human inspects: does code still satisfy? |
| `@liyi:related X` where `X` doesn't exist | **ERROR** | Unknown requirement `X`. |
| `@liyi:requirement X` with no sidecar entry | **UNTRACKED** | Requirement exists in source but is not yet tracked in `.liyi.jsonc`. |
| Requirement with no referencing items | Informational | "Requirement `X` has no related items." |

```
$ liyi check

add_money: ✓ reviewed, current
convert_currency: ⚠ STALE — source changed since spec was written
validate_order: ⚠ unreviewed (no @liyi:intent, no reviewed:true)
add_money: ⚠ REQ CHANGED — requirement "multi-currency-addition" updated
get_name: · trivial
ffi_binding: · ignored
multi-currency-addition: ✓ requirement, tracked
```

Exit code: `--fail-on-req-changed` (default: true) — exit 1 if any reviewed spec references a requirement whose hash changed. SHIFTED spans are auto-corrected and do not trigger exit 1.

This closes the **spec rot gap**: when requirements change, the requirement hash changes, and all items with `"related"` edges to that requirement are transitively flagged. The human reviews whether the code still satisfies the updated requirement. No silent re-inference over a potentially broken implementation — the requirement text is the anchor.

### `liyi reanchor`

`source_span` is the only positional field the agent writes. `source_hash` and `source_anchor` are tool-managed — computed by `liyi reanchor` (or the linter on first run) from the actual source file. Humans never compute them by hand.

`liyi reanchor` is also the tool that populates hashes for new entries. When an agent writes a sidecar with `source_span` but no `source_hash`, running `liyi reanchor` (or `liyi check --fix`) reads the source lines, computes the SHA-256, and fills in both `source_hash` and `source_anchor`. This means a fresh agent-written sidecar is incomplete until the tool runs — by design.

For resolving CI failures without an agent pass, the `liyi reanchor` subcommand re-hashes existing spans. It accepts one or more sidecar files or directories (recursive):

```bash
$ liyi reanchor src/billing/money.rs.liyi.jsonc
  add_money [42, 58]: hash updated (source changed at same span)
  convert_currency [60, 85]: hash unchanged
$ liyi reanchor crates/          # reanchor all sidecars under crates/
$ liyi reanchor a.rs.liyi.jsonc b.rs.liyi.jsonc
```

This handles the case where code at those lines changed but lines didn't shift — the human has reviewed the change and is confirming "the intent still holds." The tool computes the new hash; the human never touches it.

If lines shifted, the span points to wrong lines. Resolution paths:

- **The agent finds it** — the standard path. The agent understands code structure, re-records the span.
- **The human specifies it** — `liyi reanchor --item add_money --span 45,61`. The human looked it up in the editor ("go to definition"), the tool computes the hash.
- **Post-MVP: `--find`** — simple heuristics (grep for `fn add_money`, `def add_money`, etc.) to locate the item and update the span. Not a parser, but covers the common case.

`liyi reanchor` is a thin wrapper on the same hashing logic used by `liyi check`. No LLM calls.

### Prescriptive specs without code

A requirement can exist before the code that satisfies it. A `@liyi:requirement` block with no `@liyi:related` references is valid — the linter tracks it, reports it as informational ("requirement `X` has no related items"), and doesn't fail. When code is written and annotated with `@liyi:related X`, the dependency edge appears and transitive staleness checking begins.

### Requirement hierarchies (advanced)

Requirements can relate to other requirements via `@liyi:related`. The linter walks edges transitively — if a parent requirement changes, all descendant requirements and their related code items are flagged.

```python
# @liyi:requirement payment-security
# All payment operations must use authenticated sessions.
# No payment endpoint may be accessible without mTLS.

# @liyi:requirement multi-currency-addition
# @liyi:related payment-security
# Same-currency addition. Must reject mismatched currencies.
```

If `payment-security` changes → `multi-currency-addition` is flagged REQ CHANGED → all code items related to `multi-currency-addition` are transitively flagged. No new syntax or linter mechanism — the existing model generalizes.

The linter detects cycles (A → B → A) and reports them as errors without looping.

**Use this sparingly.** Most teams should use flat requirements — one level of `@liyi:requirement` blocks with `@liyi:related` edges from code items. Requirement hierarchies are for organizations that already think in terms of system requirements decomposing into subsystem requirements (defense, aerospace, regulated industries). If you don't already have a requirement hierarchy, don't build one just because the tool allows it — the cascading noise from deep trees (a change at the root flags everything below) can be worse than the traceability it provides.

---

## Source-Level Intent: `@liyi:intent`

Review in 立意 has two paths: a quick sidecar approval (`"reviewed": true`) for when the agent got it right, and source-level `@liyi:intent` annotations for when the human wants to state intent explicitly. The linter considers an item reviewed if **either** path is satisfied.

The sidecar path is the default for ergonomics — zero source noise. The source path is the override for safety and precision — conspicuous in code review, with the human's own words. Teams choose their default based on trust model.

### `@liyi:intent` — source-level intent (the explicit override)

`@liyi:intent` appears in a comment on the line(s) before the item definition, followed by the intent prose:

```rust
// @liyi:intent Add two monetary amounts of the same currency.
// Must be commutative. Must reject mismatched currencies with
// an error, not a panic. Must not overflow silently.
pub fn add_money(a: Money, b: Money) -> Result<Money, CurrencyError> {
```

```python
# @liyi:intent Convert amount between currencies using the
# given rate. Rate must be positive. Result currency must
# match target.
def convert_currency(amount: Money, target: Currency, rate: float) -> Money:
```

The linter detects `@liyi:intent` inside a specced item’s `source_span` and marks that item as reviewed — the association is mechanical: if the marker’s line falls within `[span_start, span_end]`, it belongs to that item. `@liyi:intent` markers outside any spec’s span are ignored by the linter; the inferring agent may pick them up on the next inference pass at its discretion. When both `@liyi:intent` and `"reviewed": true` are present, source intent takes precedence for adversarial testing — it’s the human’s assertion, which may differ from the agent’s inference.

### `@liyi:intent=doc` — the docstring shorthand

When the docstring already captures intent, a separate `@liyi:intent` block is redundant. The `=doc` variant says "my docstring is my intent":

```rust
// @liyi:intent=doc
/// Add two monetary amounts of the same currency.
/// Must be commutative. Must reject mismatched currencies with
/// an error, not a panic. Must not overflow silently.
pub fn add_money(a: Money, b: Money) -> Result<Money, CurrencyError> {
```

```python
# @liyi:intent=doc
def convert_currency(amount: Money, target: Currency, rate: float) -> Money:
    """Convert amount between currencies using the given rate.

    Rate must be positive. Result currency must match target.
    """
```

The linter treats `@liyi:intent=doc` identically to `@liyi:intent <prose>` — the item is reviewed. The adversarial testing agent reads the docstring as the authoritative intent. One annotation, zero duplication.

Multilingual aliases: `@立意:意图` / `@liyi:intent` and `@立意:意图=文档` / `@liyi:intent=doc`. The `=doc` / `=文档` suffix is part of the marker, not a separate parameter — the linter matches the full string.

### `"=doc"` in the sidecar — the agent equivalent

The agent can also signal "the docstring captures intent" by writing `"intent": "=doc"` in the sidecar:

```jsonc
{
  "item": "convert_currency",
  "intent": "=doc",
  "source_span": [60, 85]
}
```

Meaning: "I read the docstring and it already says what this item should do. I’m not restating it."

The `=` prefix marks this as a symbolic reference, not prose — it can’t collide with actual intent text. The linter treats `"=doc"` the same as any other `"intent"` value for staleness purposes (the hash covers the span including the docstring). The adversarial testing agent reads the source span and finds the docstring itself.

This gives agents a DRY option: well-documented code gets `"intent": "=doc"` instead of a redundant paraphrase. Poorly documented or undocumented code gets explicit intent prose. The distinction is useful information for reviewers — `"=doc"` signals "the docstring is adequate," explicit prose signals "it wasn’t, so I stated intent myself."

| Who | Form | Meaning |
|---|---|---|
| Agent | `"intent": "=doc"` in sidecar | "The docstring captures it" |
| Agent | `"intent": "<prose>"` in sidecar | "Here’s what I infer it should do" |
| Human | `@liyi:intent=doc` in source | "I confirm the docstring is my intent" |
| Human | `@liyi:intent <prose>` in source | "Here’s what I say it should do" |
| Human | `"reviewed": true` in sidecar | "The agent’s inference is correct" |

### Why two review paths

The source-level path (`@liyi:intent`) and the sidecar path (`"reviewed": true`) serve different needs:

- **No `"reviewed"` field to forge.** The security concern — an agent writing `"reviewed": true` directly — dissolves. Review is visible in source diffs, attributable via `git blame` on the actual source file, and covered by CODEOWNERS. An agent would have to write `@liyi:intent` in source to fake review, which is conspicuous in code review.
- **Merge conflicts become trivial.** If humans never touch the sidecar, it's fully regenerable — `liyi reanchor` after merge, zero human intervention. Same model as `Cargo.lock` or `pnpm-lock.yaml`.
- **Review is visible where it matters.** A `@liyi:intent` block above a function is visible in the normal code review flow — no need to open a separate `.liyi.jsonc` diff tab.

The sidecar retains: `"item"`, `"reviewed"` (optional, defaults to `false`), `"intent"` (the agent's *inferred* intent or `"=doc"`), `"source_span"`, `"source_hash"`, `"source_anchor"`, `"confidence"`, and `"related"`. The agent writes `"item"`, `"intent"`, `"source_span"`, `"confidence"`, and `"related"`. The tool fills in `"source_hash"` and `"source_anchor"`. The human (or CLI/IDE) sets `"reviewed": true`.

### Divergence between source and sidecar intent

The source `@liyi:intent` is the human's assertion. The sidecar `"intent"` is the agent's inference. They may differ — the human may have corrected, refined, or overridden the agent's inference. This is expected. The adversarial testing agent uses the source-level intent (the human-reviewed version), not the sidecar copy.

If the agent re-infers and its new `"intent"` text differs substantially from the source `@liyi:intent`, that's an informational signal (the code may have drifted from the human's stated intent), not an error.

### The lifting workflow

In steady state with IDE integration:

1. Agent infers intent → writes sidecar (`"reviewed"` absent or `false`, no `@liyi:intent` in source yet).
2. LSP shows inferred intent inline (hover or code action).
3. Human accepts one of:
   - **Quick approval**: IDE sets `"reviewed": true` in sidecar (code action: "Accept inferred intent"). Zero source change.
   - **Explicit override**: IDE inserts `@liyi:intent` annotation in source (code action: "Assert intent in source"). Human can edit the prose.
   - **Docstring shorthand**: Human writes `@liyi:intent=doc` if the docstring suffices.
4. Linter sees either path → item is reviewed.

Without IDE integration, the human edits the sidecar directly (setting `"reviewed": true`) or adds `@liyi:intent` in source. Both work. A dedicated `liyi review` CLI subcommand is a post-MVP convenience.

### Source file noise

Every reviewed item that uses `@liyi:intent` gets a comment annotation. For a file with 15 functions, that could be 15 intent blocks if all use source-level intent. Some teams will find this chatty. Counter-arguments:

- Many codebases already have function-level doc comments; `@liyi:intent` is a structured version of what good doc comments already do.
- `@liyi:intent=doc` collapses to one line for well-documented code.
- Teams that prefer minimal source noise can use sidecar review (`"reviewed": true`) as the default path — no source annotations except for critical-path items where explicit intent matters.
- The alternative — reviewing intent in JSON diffs alongside `source_span` and `source_hash` noise — is worse.

---

## Exclusion and triviality

### Item-level: inline annotations

Three annotations, on the line before the item definition:

- **`@liyi:ignore`** — the function is deliberately excluded from the convention. Don’t infer intent, don’t report it. Use for internal helpers, legacy functions that won’t be touched.
- **`@liyi:trivial`** — the function’s intent is self-evident from its signature. A spec would add no value. Use for simple getters, setters, one-line wrappers. Applied by the agent during inference.
- **`@liyi:nontrivial`** — a human override: “this looks trivial but I want a spec.” The agent must infer a spec and not re-classify as trivial. The linter treats it the same as an unannotated item.

```python
# @liyi:ignore: internal helper, not part of public contract
def _rebalance_tree():
    ...

# @liyi:trivial
def get_name(self):
    return self.name
```

```rust
// @liyi:ignore: legacy error path, scheduled for removal
fn old_error_handler() { ... }

// @liyi:trivial
fn name(&self) -> &str { &self.name }
```

The linter treats `@liyi:ignore` and `@liyi:trivial` the same: no spec required. The distinction is for humans and agents — `@liyi:trivial` is an intentional classification (“I looked at this and it’s not worth speccing”), `@liyi:ignore` is an opt-out (“this doesn’t participate”).

**Justification convention.** `@liyi:ignore` accepts an optional reason after the colon: `@liyi:ignore: <reason>`. The linter does not enforce this in 0.1, but teams are encouraged to include one — without a reason, `@liyi:ignore` is a black hole for future readers. `liyi check --require-ignore-reason` can enforce non-empty justifications in a later release.

During inference, the agent should annotate trivial items with `@liyi:trivial` rather than silently skipping them. This makes the classification visible and reviewable. If a reviewer disagrees, they replace it with `@liyi:nontrivial` — the agent then infers a spec on the next pass and won’t override with `@liyi:trivial`. The linter treats `@liyi:nontrivial` the same as an unannotated item: a spec is required.

### Multilingual annotations

Annotation markers (`@liyi:ignore`, `@liyi:trivial`, `@liyi:nontrivial`, `@liyi:module`, `@liyi:intent`) accept aliases in other languages. The linter maintains a static alias table — a hardcoded set of strings that all map to the same meaning. No alias is privileged; Chinese is listed first to reflect the project's origin, not to imply preference:

| 中文 | English | Español | 日本語 | Français | 한국어 | Português |
|---|---|---|---|---|---|---|
| `@立意:忽略` | `@liyi:ignore` | `@liyi:ignorar` | `@立意:無視` | `@liyi:ignorer` | `@립의:무시` | `@liyi:ignorar` |
| `@立意:显然` | `@liyi:trivial` | `@liyi:trivial` | `@立意:自明` | `@liyi:trivial` | `@립의:자명` | `@liyi:trivial` |
| `@立意:并非显然` | `@liyi:nontrivial` | `@liyi:notrivial` | `@立意:非自明` | `@liyi:nontrivial` | `@립의:비자명` | `@liyi:nãotrivial` |
| `@立意:模块` | `@liyi:module` | `@liyi:módulo` | `@立意:モジュール` | `@liyi:module` | `@립의:모듈` | `@liyi:módulo` |
| `@立意:需求` | `@liyi:requirement` | `@liyi:requisito` | `@立意:要件` | `@liyi:exigence` | `@립의:요건` | `@liyi:requisito` |
| `@立意:有关` | `@liyi:related` | `@liyi:relacionado` | `@立意:関連` | `@liyi:lié` | `@립의:관련` | `@liyi:relacionado` |
| `@立意:意图` | `@liyi:intent` | `@liyi:intención` | `@立意:意図` | `@liyi:intention` | `@립의:의도` | `@liyi:intenção` |

This follows the Cucumber/Gherkin approach: Gherkin accepts `Given`/`Dado`/`假如` as equivalent keywords via a static lookup table. No locale detection, no runtime configuration, no user preference — the linter simply accepts any known alias. The table is a const array in source, under 100 entries, community-extensible via PR.

Both prefix forms are accepted: `@立意:忽略` (fully localized) and `@liyi:忽略` (ASCII prefix, localized annotation). The linter matches the full string against the alias set regardless of prefix. Half-width and full-width punctuation are equivalent — see *Marker normalization* in the CI Linter section.

The `intent` field in `.liyi.jsonc` and `@liyi:module` prose are already language-agnostic — they’re NL processed by LLMs, which handle any language natively. A Japanese team writes `"intent": "同じ通貨の2つの金額を加算する。交換法則を満たすこと。"` and everything works: the linter doesn’t read intent prose, the testing agent does. Multilingual annotations complete the picture — every human-facing surface of the convention can be used in any supported language.

### File-level: `.liyiignore`

Inline annotations don’t work for files you can’t modify — generated code, vendored dependencies, protobuf bindings. The generator will overwrite any annotation you add.

For these, use a `.liyiignore` file in the directory:

```gitignore
# Auto-generated protobuf bindings
*.pb.go
*.pb.rs

# Vendored dependencies
vendor/
```

Semantics follow `.gitignore`: patterns are relative to the directory containing the `.liyiignore` file, and parent `.liyiignore` patterns cascade into subdirectories (just as `.gitignore` does). The linter skips matched files entirely — no spec required, no reporting. Each directory can have its own `.liyiignore` to add more patterns or negate inherited ones.

To ignore an entire directory, a single `*` in `.liyiignore` suffices.

Clean separation: inline annotations (`@liyi:ignore`, `@liyi:trivial`) for item-level control in files you own; `.liyiignore` for file-level control over files you don’t.
---

## The CI Linter: `liyi check`

The core deliverable. Agent instructions tell agents what to do; the linter reports stale and unreviewed specs.

Agent instructions are probabilistic — in practice, compliance is inconsistent and unverifiable. The linter is deterministic: it reads files, computes hashes, and exits 0 or 1.

### File discovery

The linter needs to know which files are in scope. Two declarative mechanisms, no config file:

- **Include**: CLI positional args. `liyi check src/ lib/` means "only check items in these subtrees." Default with no args: CWD, recursive.
- **Exclude**: `.gitignore` is always respected (the Rust `ignore` crate handles this natively). `.liyiignore` adds project-specific exclusions on top.

The linter resolves `"source"` paths in `.liyi.jsonc` relative to the repository root, discovered by walking up from CWD to find `.git/`. A `--root` flag overrides this for non-git repositories or unusual layouts.

**Requirement discovery is project-global.** Positional args scope which items are checked (pass 2), not which requirements are indexed. Pass 1 always walks the full project root to discover all `@liyi:requirement` markers, regardless of CLI positional args. This ensures that `liyi check src/billing/` can resolve `@liyi:related` edges pointing to requirements defined in `docs/requirements.md` or any other location in the repo.

This handles the common case without configuration. `.gitignore` already excludes `node_modules/`, `.venv/`, `target/`, `__pycache__/`, `build/`, etc. `.liyiignore` picks up the rest — checked-in vendored code, generated protobuf bindings, FFI stubs.

### Scope: staleness and review, not coverage

In 0.1, the linter checks the *quality* of existing specs, not the *coverage* of the codebase:

- **Staleness**: for each spec, hash the source lines at `source_span` and compare against `source_hash`. Mismatch → stale.
- **Requirement tracking**: discover `@liyi:requirement` markers, hash their content, resolve `"related"` edges in `.liyi.jsonc`. If a requirement's hash changed, all specs with edges to it are flagged.
- **Review status**: report specs where the item has neither a `@liyi:intent` annotation in source nor `"reviewed": true` in the sidecar — the item is unreviewed. Optionally fail CI via `--fail-on-unreviewed`.
- **Exclusion**: respect `@liyi:ignore`, `@liyi:trivial` annotations and `.liyiignore` patterns.

What it does *not* do in 0.1: detect items that have no spec and no annotation. That requires identifying item definitions in source — either via regex heuristics or tree-sitter. The agent handles coverage during inference; the linter enforces quality of what the agent wrote. Item detection can be added when demand justifies it or when another feature (e.g., a coverage report) needs it anyway.

**Caveat: green linter ≠ full coverage.** A passing `liyi check` means every *existing* spec is current and not stale. It does not mean every item in the codebase has a spec. Files the agent never touched will have no `.liyi.jsonc` at all, and the linter won't complain. Teams should be aware that `liyi check` guards quality, not completeness — the agent's coverage during inference is the first line of defense. Imperfect coverage is strictly better than no coverage; the convention makes intent *possible* to persist, not *guaranteed* to be persisted.

### What it does

```
$ liyi check

add_money: ✓ reviewed, current
convert_currency: ⚠ STALE — source changed since spec was written
validate_order: ⚠ unreviewed
add_money: ⚠ REQ CHANGED — requirement "multi-currency-addition" updated
process_refund: ↕ SHIFTED [120,135]→[123,138] — span auto-corrected
refund-policy: ⚠ UNTRACKED — requirement exists in source but not in sidecar
multi-currency-addition: ✓ requirement, tracked
get_name: · trivial
ffi_binding: · ignored
```

Exit codes: 0 = clean, 1 = check failures (stale, unreviewed, or diverged specs), 2 = internal error (malformed JSONC, missing files). CI flags control which conditions trigger exit 1:
- `--fail-on-stale` (default: true) — exit 1 if any reviewed spec's source hash doesn't match
- `--fail-on-unreviewed` (default: false) — exit 1 if specs exist without `@liyi:intent` in source or `"reviewed": true` in sidecar
- `--fail-on-req-changed` (default: true) — exit 1 if any reviewed spec references a requirement whose hash changed

### What it doesn't do

- No LLM calls. No API keys. No network access.
- No test generation. No spec inference.
- No tree-sitter in 0.1. It reads line ranges from `source_span`, hashes them, and compares. Simple regex for `@liyi:ignore`, `@liyi:trivial`, `@liyi:intent`, `@liyi:requirement`, and `@liyi:related`.

### Marker normalization (half-width / full-width equivalence)

CJK input methods default to full-width punctuation. Japanese IME often produces full-width `＠` as well. Requiring users to switch to half-width mode for every annotation is a constant friction — and a guaranteed source of "why doesn't the linter see my marker" bug reports.

The linter normalizes four structural characters before pattern-matching:

| Half-width | Full-width | Role |
|---|---|---|
| `@` (U+0040) | `＠` (U+FF20) | Marker prefix |
| `:` (U+003A) | `：` (U+FF1A) | Namespace separator |
| `(` (U+0028) | `（` (U+FF08) | Name delimiter open |
| `)` (U+0029) | `）` (U+FF09) | Name delimiter close |

All of the following are equivalent:
- `@liyi:requirement(auth-check)`
- `＠liyi：requirement（auth-check）`
- `@立意：需求（認証チェック）`
- `＠立意:需求(認証チェック)`

**Implementation approach: normalize-then-match.** The linter runs a single normalization pass on each scanned line — replacing the four full-width characters with their half-width equivalents — before applying the marker regex. This is a four-entry `str::replace` chain (or a single `translate` table), not a regex concern. The normalization happens only on lines being scanned for markers, not on the entire file, so it has negligible cost. The alias lookup table stores only half-width forms; normalization ensures they match regardless of what the user typed.

This is strictly more robust than the alternative (doubling every regex to accept both forms), keeps the alias table simple, and confines the full-width concern to one function in the lexer.

### Post-MVP: `liyi triage` — agent-driven staleness assessment

When `liyi check` reports stale items, the next question is: *does it matter?* A variable rename is cosmetic; a new code path is semantic; a contradiction to declared intent is a bug. Answering that question requires LLM reasoning — but `liyi` itself never calls an LLM.

**Architectural principle: `liyi` is infrastructure; the agent is the brain.** The binary provides the index (hashes, spans, graphs), the schema (sidecar format, triage report format), and the ratchet (CI enforcement). The agent provides the reasoning (intent inference, triage assessment, challenge verdicts, adversarial tests). The contract between them is structured JSON.

By 2026, every developer workflow that would adopt 立意 already has a model configured — Copilot in VS Code, Cursor, aider, CI pipelines with Codex, custom agent setups. Asking the user to configure *another* set of API keys and model preferences inside `liyi` is friction that buys nothing. The agent that's already running can do the triage.

**How triage works.** Three paths to the same report:

| Path | Who assembles the prompt? | Who calls the LLM? | Who validates? |
|---|---|---|---|
| IDE agent | Agent (from AGENTS.md instruction) | Agent | `liyi triage --validate` |
| CI with `--prompt` | `liyi triage --prompt` | Team's LLM wrapper | `liyi triage --validate` |
| Custom pipeline | Team's own template | Team's own code | `liyi triage --validate` |

All three paths converge on the same report schema and the same `--validate` / `--apply` commands.

**`liyi triage` subcommands:**

| Subcommand | What it does | Calls LLM? |
|---|---|---|
| `liyi triage --prompt` | Assemble a self-contained prompt from `liyi check --json` output — includes stale items with full context, the triage schema, assessment instructions, and output format spec. Print to stdout. | No |
| `liyi triage --validate <file>` | Validate an agent-produced triage report against the schema; check that every assessed item corresponds to a real stale item | No |
| `liyi triage --apply [file]` | Auto-reanchor items with `cosmetic` verdict; present `semantic` items with suggested intents; flag `intent-violation` items for human review | No |
| `liyi triage --summary [file]` | Print human-readable summary of a triage report | No |

The `--prompt` flag is the bridge for CI/script pipelines that have an `llm` CLI or API wrapper but no full agentic framework:

```bash
liyi triage --prompt | llm-call > .liyi/triage.json
liyi triage --validate .liyi/triage.json
liyi triage --apply .liyi/triage.json
```

`liyi` knows the schema, knows what context to include, knows what output format to request. The team routes it to whatever model they have. The binary makes zero LLM calls.

**`liyi check --json` output.** The triage workflow depends on a machine-readable output from `liyi check` that provides full context for each stale item:

```jsonc
{
  "version": "0.1",
  "root": ".",
  "stale_items": [
    {
      "source": "src/billing/money.rs",
      "sidecar": "src/billing/money.rs.liyi.jsonc",
      "item": "Currency::convert",
      "source_span": [60, 85],
      "intent": "Must reject mismatched currencies with ConversionError",
      "reviewed": true,
      "old_hash": "sha256:a1b2c3...",
      "new_hash": "sha256:d4e5f6...",
      "new_source": "pub fn convert(...) {\n    ...\n}",
      "related": {
        "multi-currency-addition": {
          "recorded_hash": "sha256:b7c8d9...",
          "current_hash": "sha256:b7c8d9...",
          "changed": false
        }
      },
      "depended_on_by": [
        { "source": "src/billing/money.rs", "item": "Money::add" },
        { "source": "src/portfolio/rebalance.rs", "item": "Portfolio::rebalance" }
      ]
    }
  ]
}
```

This is the full context an assessor needs. The agent (or script, or CI wrapper) reads this, reasons about each item, and produces the triage report.

**Triage report schema** (`.liyi/triage.json`):

```jsonc
{
  "version": "0.1",
  "generated": "2026-03-06T12:00:00Z",
  "model": "claude-sonnet-4-20260514",
  "root": ".",
  "summary": {
    "total_stale": 34,
    "cosmetic": 28,
    "semantic": 4,
    "intent_violation": 2,
    "unassessed": 0,
    "impacted_transitively": 7
  },
  "items": [
    {
      "source": "src/billing/money.rs",
      "item": "Currency::convert",
      "source_span": [60, 85],
      "verdict": "intent-violation",
      "confidence": 0.92,
      "change_summary": "Added a fallback path that silently returns 0.0 on currency mismatch.",
      "invariant_summary": "The rejection-on-mismatch behavior is removed. All other logic is unchanged.",
      "reasoning": "The declared intent says 'must reject mismatched currencies with ConversionError'. The new code returns Money { cents: 0, ... } on mismatch. This directly contradicts the intent.",
      "action": "fix-code-or-update-intent",
      "suggested_intent": null,
      "impact": [
        {
          "source": "src/billing/money.rs",
          "item": "Money::add",
          "relationship": "related:multi-currency-addition",
          "impact_summary": "Money::add's intent assumes convert rejects mismatches."
        }
      ]
    }
  ]
}
```

**Verdict enum:**

| Verdict | Meaning | Default action |
|---|---|---|
| `cosmetic` | Variable rename, reformatting, comment edit — no behavioral change | Auto-reanchor (no human review needed) |
| `semantic` | Code legitimately evolved — intent is stale but code is correct | Update intent (human reviews suggested intent) |
| `intent-violation` | Code contradicts declared intent — either code is wrong or intent is wrong | Fix code or update intent (human decides) |
| `unclear` | LLM can't determine with sufficient confidence | Manual review (human decides) |

**Per-item fields:**

| Field | Type | Description |
|---|---|---|
| `source` | string | Repo-relative source path |
| `item` | string | Item name (matches sidecar) |
| `source_span` | [int, int] | Current span (from sidecar) |
| `verdict` | enum | cosmetic / semantic / intent-violation / unclear |
| `confidence` | float | 0–1, model's self-assessed confidence in the verdict |
| `change_summary` | string | What changed in the code (1–2 sentences) |
| `invariant_summary` | string | What stayed the same (1–2 sentences) |
| `reasoning` | string | Why the verdict was assigned (2–3 sentences, citable in reviews) |
| `action` | enum | auto-reanchor / update-intent / fix-code-or-update-intent / manual-review |
| `suggested_intent` | string? | Proposed new intent text (only for `semantic` verdict) |
| `impact` | array | Transitively affected items via `related` graph |

**Consumers and their read patterns:**

| Consumer | What it reads | How |
|---|---|---|
| CI/PR comment | `summary` + items with verdict ≠ cosmetic | Format as markdown table in PR comment |
| Dashboard | `summary` for aggregate view; items for drill-down | Read JSON, render charts / tables |
| LSP | Items for inline diagnostics at `source_span` | Watch `.liyi/triage.json`, map items to diagnostic locations |
| `liyi triage --apply` | Items with `verdict: cosmetic` | Auto-reanchor those items (write back to sidecars) |
| Agent (next session) | `suggested_intent` for items with `verdict: semantic` | Read triage, propose intent updates in sidecar |
| Human (terminal) | Formatted summary + triage table | `liyi triage --summary`; `--json` for raw |

**Why the LLM is not in the binary.** Building LLM calls into `liyi` would require API key management, provider abstraction (OpenAI, Anthropic, Bedrock, Vertex, local models...), HTTP client + TLS, rate limit handling, token budgeting, and retry logic. It would bloat a 500-line binary with complexity that the agentic framework already solved. The binary stays deterministic, offline, and small. The reasoning lives where the model access already is.

**Triage workflow:**

```
liyi check              # deterministic, no LLM, produces diagnostics
    │
    │ stale items identified (exit code 1)
    ▼
agent assesses           # agent reads stale items, reasons about each
    │                    # (or: liyi triage --prompt | llm-call)
    │ writes .liyi/triage.json
    ▼
liyi triage --apply     # auto-reanchors cosmetic items
    │                   # prints remaining items needing human review
    ▼
human reviews           # reads triage report or PR comment
                        # accepts suggested intents or fixes code
```

This replaces the previously described `--smart` flag. The split is cleaner: `liyi check` is always deterministic and offline; `liyi triage` consumes an agent-produced report. No mode mixing. Each command has one job.

**Relationship to challenge.** Triage and challenge are complementary. Triage answers "what changed and does it matter?" — a reactive assessment triggered by staleness. Challenge answers "does this code actually satisfy this intent?" — a proactive verification triggered by human curiosity. Both are agent-driven, both produce structured output, and neither is built into the binary. Triage operates on stale items (batch, CI-oriented); challenge operates on any specced item (on-demand, developer-oriented).

### `--fix` mode

`liyi check` is read-only by default — it reports diagnostics and exits 0, 1, or 2. `liyi check --fix` is the side-effecting variant:

- Fills in missing `source_hash` and `source_anchor` for specs that have `source_span` but no hash (fresh agent-written sidecars).
- Auto-corrects SHIFTED spans (updates `source_span`, recomputes hash and anchor).
- Attempts tree-path re-resolution **before** validating span boundaries — if `tree_path` is set and the current `source_span` is past EOF or otherwise invalid, the tool resolves via tree-sitter first and replaces the span. This handles file truncation (e.g., `cargo fmt` removing lines) gracefully.

`--fix --dry-run` shows what `--fix` would change without writing any files. Each correction is printed as a diff-like line (`item: [old_span] → [new_span], hash updated`). This lets users preview mechanical corrections before committing them.

`--fix` never modifies `"intent"`, `"reviewed"`, `"related"`, or any human-authored field. It only writes tool-managed fields. This is the same contract as `eslint --fix` or `cargo clippy --fix` — mechanical corrections, no semantic changes.

**Semantic drift protection.** When `tree_path` resolves an item to a new span, `--fix` compares the hash at the new location against the recorded `source_hash`. If the content is unchanged (pure positional shift), the span, hash, and anchor are all updated — this is a safe mechanical correction. If the content at the new span also changed (semantic drift), `--fix` updates `source_span` to track the item's current location but does **not** rewrite `source_hash` — the spec remains stale so the next `liyi check` flags it for human review. This prevents `--fix` from silently blessing semantic changes that may invalidate the declared intent.

The shift heuristic (non-`tree_path` fallback) is inherently safe — it only matches when the *exact same content* is found at an offset — so no additional protection is needed there.

`liyi reanchor` remains as the explicit manual tool for targeted re-hashing (e.g., `liyi reanchor --item add_money --span 45,61`). `--fix` is the batch equivalent for CI and post-merge workflows.

### Implementation

~2000–3000 lines of Rust across two crates (`liyi` library + `liyi-cli` binary), organized as a Cargo workspace under `crates/`. Core check logic is ~500 lines; the remainder covers CLI, diagnostics, span-shift detection, `--fix` write-back, marker normalization, `reanchor`, and `approve`. Dependencies: `clap`, `serde`, `serde_json`, `sha2`, `ignore`.

No config file reader. `.liyiignore` handles file exclusion; config-based ignore patterns are a post-MVP consideration.

**Two-pass design.** The `"related"` hash-comparison model requires the linter to resolve requirement names globally. The linter cannot validate `"related"` edges in a single pass — it must first discover all `@liyi:requirement` markers and hash them, then validate edges in item specs on a second pass (or a deferred validation queue). This is straightforward but means naive single-pass implementations won't work.

**Performance.** The linter's work is directory walking + line slicing + SHA-256 hashing — all I/O-bound and parallelizable. A monorepo with 10,000 source files and proportional sidecars should complete in seconds. The `ignore` crate already handles `.gitignore`/`.liyiignore` filtering efficiently.

**Merge conflicts in sidecars.** Two branches editing the same source file will both update `source_span`/`source_hash` in the co-located `.liyi.jsonc`, causing a merge conflict. Resolution: `liyi reanchor` after merge, same model as `pnpm install` / `yarn install` resolving lockfile conflicts — re-run the tool, the derived fields are recomputed from the merged source. True intent-text conflicts (both branches edited the same item's `intent` prose) are rare and handled by normal git conflict resolution.

### Diagnostic catalog

Every diagnostic the linter can emit, with its severity, exit code contribution, and message template:

| Condition | Severity | Exit code | Message template |
|---|---|---|---|
| Spec current and reviewed | info | 0 | `<item>: ✓ reviewed, current` |
| Spec current but unreviewed | warning | 1 if `--fail-on-unreviewed` | `<item>: ⚠ unreviewed (no @liyi:intent, no reviewed:true)` |
| Source hash mismatch (stale) | warning | 1 if `--fail-on-stale` | `<item>: ⚠ STALE — source changed since spec was written` |
| Source hash found at offset (shifted) | info | 0 (auto-corrected with `--fix`) | `<item>: ↕ SHIFTED [old]→[new] — span auto-corrected` |
| Referenced requirement hash changed | warning | 1 if `--fail-on-req-changed` | `<item>: ⚠ REQ CHANGED — requirement "<name>" updated` |
| `@liyi:related X` where X doesn't exist | error | 1 | `<item>: ✗ ERROR — unknown requirement "<name>"` |
| Requirement exists but untracked in sidecar | warning | 0 | `<name>: ⚠ UNTRACKED — requirement exists in source but not in sidecar` |
| Requirement with no referencing items | info | 0 | `<name>: · requirement has no related items` |
| Item annotated `@liyi:trivial` | info | 0 | `<item>: · trivial` |
| Item annotated `@liyi:ignore` | info | 0 | `<item>: · ignored` |
| `source_span` past EOF | error | 1 | `<item>: ✗ source_span [s, e] extends past end of file (<n> lines)` |
| Inverted or zero-length `source_span` | error | 1 | `<item>: ✗ invalid source_span [e, s]` or `empty source_span [s, s]` |
| Malformed `source_hash` | error | 1 | `<item>: ✗ malformed source_hash` |
| Duplicate item + span | warning | 0 | `<item>: ⚠ duplicate entry (same item name and source_span)` |
| Source file deleted / not found | error | 1 | `<file>: ✗ source file not found — spec is orphaned` |
| Malformed JSONC | error | 2 | `<file>: ✗ parse error: <detail>` |
| Unknown `"version"` | error | 2 | `<file>: ✗ unknown version "<v>"` |
| Cycle in requirement hierarchy | error | 1 | `<name>: ✗ requirement cycle detected: <path>` |
| Ambiguous sidecar (duplicate naming) | warning | 0 | `<file>: ⚠ ambiguous sidecar — both <correct>.liyi.jsonc and <wrong>.liyi.jsonc exist. Only <correct>.liyi.jsonc will be used.` |

When both `money.rs.liyi.jsonc` (canonical) and `money.liyi.jsonc` (wrong) exist, the linter uses only the canonical form and warns about the other. The canonical name is always `<source_filename>.liyi.jsonc` — derived by appending `.liyi.jsonc` to the full source filename.

Exit code 2 (internal error) takes precedence over exit code 1 (check failure). Within exit code 1, any single triggering condition is sufficient.

### Testing strategy for the linter

The linter is tested at three levels:

- **Golden-file tests.** A `tests/fixtures/` directory contains small synthetic repos — source files, `.liyi.jsonc` sidecars, `.liyiignore` files, annotation markers — with a corresponding `.expected` file containing the expected `liyi check` output. Each fixture exercises one scenario (stale, shifted, unreviewed, orphaned, malformed, etc.). The test runner runs `liyi check` on each fixture and diffs against expected output. This is the primary regression guard.
- **Property-based tests for span-shift detection.** The ±100-line scan window and delta-propagation logic are exercised with randomized inputs: insert N lines at random positions, verify that the linter correctly identifies SHIFTED spans and auto-corrects them. This catches off-by-one errors in the heuristic.
- **Dogfooding.** The 立意 project's own source has `.liyi.jsonc` specs and `@liyi:module` markers. CI runs `liyi check` on the linter's own codebase. This is both a test and a demonstration.

---

## The Agent Skill

The agent instructions define the protocol. The CI linter enforces it.

### Minimal instruction (~12 lines, for any AGENTS.md)

```markdown
## 立意 (Intent Specs)

When writing or modifying code:
1. For each non-trivial item (function, struct, macro invocation, decorated endpoint, etc.), infer what it SHOULD do (not what it does). Write intent to a sidecar file named `<source_filename>.liyi.jsonc` (e.g., `money.rs` → `money.rs.liyi.jsonc`). Record `source_span` (start/end lines). Do not write `source_hash` or `source_anchor` — the tool fills them in. Do not write `"reviewed"` — that is set by the human via CLI or IDE. Use `"intent": "=doc"` only when the docstring contains behavioral requirements (constraints, error conditions, properties), not just a functional summary — a docstring that says "Returns the sum" is not adequate; one that says "Must reject mismatched currencies with an error" is. For trivial items (simple getters, one-line wrappers), annotate with `@liyi:trivial` instead of writing a spec.
2. When module-level invariants are apparent, write an `@liyi:module` block — in the directory's existing module doc (`README.md`, `doc.go`, `mod.rs` doc comment, etc.) or in a dedicated `LIYI.md`. Use the doc markup language's comment syntax for the marker.
3. If a source item has a `@liyi:related <name>` annotation, record the dependency in `.liyi.jsonc` as `"related": {"<name>": null}`. The tool fills in the requirement's current hash.
4. For each `@liyi:requirement <name>` block encountered, ensure it has a corresponding entry in the co-located `.liyi.jsonc` with `"requirement"` and `"source_span"`. (The tool fills in `"source_hash"`.)
5. If a spec has `"related"` edges referencing a requirement, do not overwrite the requirement text during inference. Re-anchor the spec (update `source_span`) but preserve the `"related"` edges. Do not write `source_hash` — the tool fills it in.
6. Only generate adversarial tests from items that have a `@liyi:intent` annotation in source or `"reviewed": true` in the sidecar (i.e., human-reviewed intent). When `@liyi:intent` is present in source, use its prose (or the docstring for `=doc`) as the authoritative intent for test generation.
7. Tests should target boundary conditions, error-handling gaps, property violations, and semantic mismatches. Prioritize tests a subtly wrong implementation would fail.
8. Skip items annotated with `@liyi:ignore` or `@liyi:trivial`, and files matched by `.liyiignore`. Respect `@liyi:nontrivial` — if present, always infer a spec for that item and never override with `@liyi:trivial`.
9. Use a different model for test generation than the one that wrote the code, when possible.
10. When `liyi check` reports stale items, assess each: is the change cosmetic (rename, reformat — no behavioral change), semantic (code legitimately evolved — intent needs updating), or an intent violation (code contradicts declared intent)? Write the assessment to `.liyi/triage.json` following the triage report schema. For cosmetic changes, run `liyi triage --apply` to auto-reanchor. For semantic changes, propose updated intent in the `suggested_intent` field. For intent violations, flag for human review.
```

### Key principles

- **Adversarial, not confirmatory.** Find bugs, not confirm correctness.
- **Spec is the referee.** If the spec says one thing and the code does another, the test exposes the gap. The human decides who's right.
- **Model diversity.** Different model for tests than for code, when possible.
- **Never modify source code logic** during the protocol. Only create/update `.liyi.jsonc` files, `@liyi:module` blocks (in docs or doc comments), test files, and annotation comments (`@liyi:trivial`, `@liyi:ignore`, `@liyi:requirement`, `@liyi:related`). Annotation comments are metadata, not logic — adding them does not change program behavior.

---

## Post-MVP: IDE and Agent Integration

The file-based convention (`.liyi.jsonc`, annotation markers, `liyi check` CLI) is the foundation and works without additional tooling. IDE and agent integrations are UX layers that make the workflow faster — not prerequisites.

### LSP server

An LSP (Language Server Protocol) server wraps `liyi check` output as editor diagnostics. It works in any LSP-capable editor — VSCode, Neovim, Emacs, Helix, Zed — so the integration is not editor-specific. The VSCode extension becomes a thin LSP client (~50 lines of boilerplate).

| LSP Feature | 立意 Use |
|---|---|
| **Diagnostics** | Inline warnings at STALE, REQ CHANGED, and unreviewed sites |
| **Code Actions** | \"Accept inferred intent\" (sets `\"reviewed\": true` in sidecar), \"Assert intent in source\" (inserts `@liyi:intent`), \"Reanchor span\", \"Go to requirement\", \"Challenge\" (on-demand semantic verification via LLM) |
| **Hover** | Show the intent spec when hovering over a specced item |
| **Go to Definition** | Jump from `@liyi:related X` to the `@liyi:requirement X` block |

~200 lines on top of the linter. Depends on the protocol being stable first — the LSP server is a consumer of `liyi check`, not a parallel implementation.

### MCP server

An MCP (Model Context Protocol) server exposes structured tool-use for agents. The primary value is requirement resolution — when an agent encounters `@liyi:related multi-currency-addition`, an MCP tool returns the requirement text and location without the agent needing to grep and parse. Useful but not load-bearing: the file-based convention works without it.

Candidate tools:

| Tool | Description |
|---|---|
| `liyi_check` | Run `liyi check` on a path, return structured results (stale, reviewed, diverged) |
| `liyi_check_json` | Run `liyi check --json` — return full context for stale items, suitable for agent-driven triage |
| `liyi_reanchor` | Re-hash spans for a given file |
| `liyi_get_requirement` | Look up a named requirement — return its text, location, and current hash |
| `liyi_list_related` | List all items with `"related"` edges to a given requirement |
| `liyi_triage_validate` | Validate an agent-produced triage report against the schema |
| `liyi_triage_apply` | Apply a validated triage report — auto-reanchor cosmetic items |

The MCP tools provide *context for* reasoning and *application of* results. The reasoning itself (triage assessment, challenge verdicts) happens in the agent — which already has model access, conversation context, and the AGENTS.md instruction. This avoids duplicating LLM call logic inside the MCP server.

~100 lines wrapping the CLI. Same stability dependency as LSP — the protocol must be settled first.

### `liyi approve` — interactive review

`liyi approve` marks one or more sidecar specs as reviewed by a human. It is the primary mechanism for transitioning intent from "agent-inferred" to "human-approved."

**Interactive mode** (default when stdin is a TTY):

For each unapproved item in the target file(s), display:
- Item name + source span
- Inferred intent text
- Source code in the span
- Diff since last `source_hash`, if any

Prompt: `approve? [y]es / [n]o / [e]dit intent / [s]kip`

- **y** — set `"reviewed": true`, update `source_hash` and `source_anchor` via reanchor.
- **n** — set `"reviewed": false` (explicit rejection). Leave hash unchanged.
- **e** — open `$EDITOR` with the intent text. After save, re-display and re-prompt.
- **s** — skip without changing anything.

**Batch mode** (`--yes` or non-TTY):

```bash
liyi approve --yes src/money.rs              # approve all items in sidecar
liyi approve --yes src/money.rs "add_money"  # approve specific item
liyi approve --yes .                         # approve all sidecars under cwd
```

Sets `"reviewed": true` and reanchors without prompting.

**Flags:**
- `--yes` — non-interactive, approve all matched items.
- `--dry-run` — print what would be approved, don't write.
- `--item <name>` — filter to specific item(s) within a sidecar.

**Exit codes:** same as `liyi check`.

### `liyi init` — scaffold sidecars and agent instructions

`liyi init` bootstraps 立意 adoption for a repository or individual files.

**Repository initialization:**

```bash
liyi init              # scaffold AGENTS.md with the 立意 instruction paragraph
liyi init --force      # overwrite existing AGENTS.md
```

Appends the ~12-line agent instruction to `AGENTS.md` (creates the file if absent). Does not overwrite existing content unless `--force` is given.

**File initialization:**

```bash
liyi init src/money.rs   # create money.rs.liyi.jsonc with empty specs array
```

Creates a skeleton `.liyi.jsonc` sidecar with `version`, `source`, and an empty `specs` array. The agent (or human) populates specs afterwards.

### Challenge: on-demand semantic verification (post-MVP)

> **Note:** Challenge is explicitly deferred to post-0.1. The `liyi approve` workflow must be established first — challenge verifies edges that only exist after humans have reviewed intent.

Challenge is a human- or agent-initiated action that asks a model to verify whether an artifact satisfies its upstream — code against intent, intent against requirement, or requirement against parent requirement. Like triage, challenge follows the same architectural principle: `liyi` provides the context; the agent does the reasoning; the verdict is structured output.

It is not a source annotation or a linter marker. It works on any edge in the intent graph:

| Edge | Question | Input |
|---|---|---|
| code → intent | Does this code do what the intent says? | source span + intent prose |
| intent → requirement | Does this intent fully cover the requirement? | intent prose + requirement text |
| requirement → parent | Does this child properly decompose the parent? | child text + parent text |

The challenge agent reads both artifacts and renders a clause-by-clause verdict:

```
Challenge intent "add_money" against requirement "multi-currency-addition":

✓ Currency mismatch handling — covered
⚠ Commutativity — NOT MENTIONED in intent
⚠ Overflow behavior — NOT MENTIONED in intent

  Intent covers 1 of 3 requirement clauses.
```

```
Challenge code against intent "add_money":

✓ Currency mismatch: returns Err(CurrencyError::Mismatch) — matches
✓ Commutativity: addition is commutative (a.cents + b.cents) — matches
⚠ Overflow: uses `+` not `checked_add` — DIVERGENCE
  Intent says "must not overflow silently" but line 12 uses
  unchecked addition which will panic on overflow in debug
  and wrap in release.
```

**Integration.** In the LSP, challenge appears as a code action on specced items and on `@liyi:related` annotations. The user clicks "Challenge" → the LSP provides the context (source span, intent prose, requirement text) to the agent → the agent reasons and produces a clause-by-clause verdict → the verdict appears as inline diagnostics. In an agentic workflow, the agent runs challenge as part of its review loop — no CLI invocation needed.

**Model selection.** The "different model" principle from adversarial testing applies. If Claude wrote the code and inferred the intent, having Claude challenge it is an echo chamber. The challenge agent should ideally be a different model. Since the agent framework (not `liyi`) manages model access, model selection is a workflow concern, not a tool concern.

**What challenge is not.** It is not a replacement for the linter (deterministic, no LLM), for triage (batch assessment of stale items), or for adversarial testing (generates persistent test files). It is zero-commitment verification — challenge when you're curious, skip when you're not. No pipeline, no batch process, no test files to maintain.

**Why this matters.** Without challenge, the reviewer's options for verifying intent are: read the code (defeats the purpose of intent-level review) or trust the agent (defeats the purpose of 立意). Challenge gives a third option: ask a second model to verify. This closes the trust gap between reviewing intent and trusting it blindly. It also makes the requirements feature (Level 5) worth the investment — the payoff is no longer just "you get flagged when the requirement hash changes" but "you can verify whether your intent actually covers the requirement."

---

## Security Model

Review has two paths — `"reviewed": true` in the sidecar (quick approval) and `@liyi:intent` in source (explicit intent assertion). Each has a different trust profile.

### Threat model

立意's security model guards against **accidental staleness and oversight**, not against **deliberate forgery**.

- **In scope:** An agent writes specs. The human forgets to review them — items remain unreviewed. The linter catches this (`--fail-on-unreviewed`). Adversarial testing is the indirect defense against *careless* review — see below.
- **In scope:** Code changes silently. The spec's hash no longer matches. The linter catches this deterministically.
- **Out of scope (sidecar path):** An agent writes `"reviewed": true` on a spec it just generated. The linter reads file contents, not authorship — it can't distinguish "human set this" from "agent set this."
- **Out of scope (source path):** An agent writes `@liyi:intent` directly in source, and the human approves the PR without noticing.

### Why careless review is self-limiting (but not self-correcting)

Adversarial testing is the indirect defense against careless review. The reasoning: if a human rubber-stamps a vague or wrong intent, the adversarial tests generated from it will be weak — and that weakness is detectable. This is directionally true but not mechanically guaranteed.

**The mechanism.** An agent infers intent. The human approves without reading. The approved intent is now the authoritative source for adversarial test generation. A second model reads that intent and generates tests designed to break the implementation. If the intent is vague or tautological, the tests are weak:

| Rubber-stamped intent | Adversarial tests produced | Signal |
|---|---|---|
| "Process the data" | `assert process_data(input) is not None` | Trivially passes. No boundary conditions tested. A reviewer sees tests that prove nothing. |
| "Add two monetary amounts" (omits currency, commutativity, overflow) | Tests for basic addition only. No currency-mismatch test, no overflow test, no commutativity property test. | Missing coverage is visible to a domain expert reading the test file — but only if they read it. |
| "Adds a.cents + b.cents and returns a Money" (tautology of the code) | Tests that the code does exactly what it does. `assert add_money(a, b).cents == a.cents + b.cents` | Confirmatory, not adversarial. A reviewer may notice the tests don't challenge anything. |

**What's honest about this.** The feedback loop depends on someone *reading the tests*. If the team also rubber-stamps adversarial test output, the signal is lost — the defense has no teeth. The claim is that vague intent produces *visibly* weak tests, making the failure mode detectable. It's not that vague intent *automatically* triggers an alert.

**What's genuinely useful.** When the intent is specific and correct — "must reject mismatched currencies with an error, not a panic; must be commutative; must not overflow silently" — the adversarial tests are sharp: test with mismatched currencies, test commutativity with randomized inputs, test near `i64::MAX`. The quality difference between tests from good intent vs. tests from rubber-stamped intent is large enough to notice when the tests are generated side by side. Teams that adopt Level 6 (adversarial testing) are choosing to read those tests — the cost of that level is test review, and weak tests are the signal that the intent review was skipped.

This is a probabilistic defense, not a deterministic one. It complements the linter (deterministic, catches staleness) rather than replacing it. The document does not claim adversarial testing catches all rubber-stamping — it claims the combination of linter + adversarial testing makes careless review more costly than careful review, because careful review produces tests that actually find bugs.

### Two paths, two trust profiles

**Sidecar review (`"reviewed": true`)** is low-friction but low-conspicuousness. The `"reviewed"` flip is a one-line JSON change in a sidecar file — easy to miss in a PR diff, and in single-commit workflows the intermediate `false` state may never be a Git snapshot. This is the same "unsolvable at file level" problem discussed in earlier iterations of this design: determining "did a human actually look at this?" requires either a witnessed interaction or a multi-snapshot VCS workflow, both of which add friction that contradicts near-zero adoption cost.

**Source intent (`@liyi:intent`)** is higher-friction but high-conspicuousness. An agent forging review must write the annotation in the actual source file — visible in the primary review surface (source diffs), attributable via `git blame`, and protectable via CODEOWNERS. This is significantly harder to sneak past a reviewer than a sidecar boolean.

The hybrid model is honest about this tradeoff:

| Path | Friction | Forgery conspicuousness | Best for |
|---|---|---|---|
| `"reviewed": true` in sidecar | Low (one click / CLI) | Low (sidecar diff) | High-trust teams, routine approvals |
| `@liyi:intent` in source | Medium (write annotation) | High (source diff) | Critical-path items, security-sensitive code |

Teams choose their default. `--fail-on-unreviewed` treats both paths as equivalent — the linter doesn't force one over the other — but a team can adopt CODEOWNERS on `.liyi.jsonc` to require human approval for sidecar review changes, achieving similar conspicuousness without source annotation noise.

### Practical mitigations

- **Branch protection rules.** Require PR approval before merge. Both `@liyi:intent` lines in source and `"reviewed": true` flips in sidecars are visible in PR diffs.
- **CODEOWNERS on `.liyi.jsonc`.** For teams that use sidecar review as the default path, assigning sidecar files to specific reviewers ensures a human must approve `"reviewed": true` changes. This brings sidecar review close to source-level conspicuousness without annotation noise.
- **CODEOWNERS on source files.** Covers the `@liyi:intent` path naturally — source files already have owners.

These are existing VCS-level controls. The linter does not replicate them — it provides the artifacts that those controls govern.

---

## Ecosystem Dependencies

The core value proposition — agents write specs alongside code — depends on agents reliably following the AGENTS.md instruction. This is an ecosystem dependency, not a design flaw.

Agent instruction compliance in 2026 is inconsistent. Different agent platforms read different instruction files (AGENTS.md, `.github/copilot-instructions.md`, `.cursor/rules/`, etc.), and compliance varies by model, provider, and context window pressure. If a major agent platform stops reading AGENTS.md or ignores the instruction, Level 0 of the adoption ladder breaks.

The progressive adoption model mitigates this: the linter (Level 3) works regardless of agent compliance — a human can write `.liyi.jsonc` files by hand, and `liyi check` will verify them. But the efficiency argument ("the agent writes specs for free alongside code") depends on the agent actually doing it.

This is the same class of dependency every AGENTS.md-based convention shares. 立意 doesn't introduce it; it inherits it. The linter is the hedge — even if agents become unreliable instruction followers, the convention and linter remain useful for human-authored specs.

---

## Build Effort (for this project)

This section estimates the effort to *build* 立意 itself — the linter, the convention docs, the demo repo. This is not the adopter's cost; adopters install the linter and copy the agent instruction (see Adoption Story below).

| Deliverable | Effort (human) | Effort (with agent) |
|---|---|---|
| Agent instruction (AGENTS.md paragraph) | 1 hour | 15 minutes |
| `@liyi:module` convention + examples | 30 minutes | 10 minutes |
| `.liyi.jsonc` examples for a demo repo | 1–2 hours | 20 minutes |
| CI linter (`liyi check` + `liyi reanchor` + `liyi approve` + `liyi init`, ~2000–3000 lines) | 3–5 days | 2–4 hours |
| Blog post explaining the practice | 1 day | 2–3 hours |
| **Total** | **3–5 days** | **Half a day** |

---

## What This Is

- A **CI linter** — `liyi check` + `liyi reanchor`, ~500–800 lines. The enforcement mechanism.
- A **spec convention** — `@liyi:module` blocks (module intent) + `@liyi:requirement` blocks (named requirements) + `.liyi.jsonc` (item-level intent and requirement tracking, JSONC).
- A **dependency model** — `@liyi:related` edges from code items to named requirements, with transitive staleness.
- A **triage protocol** (post-MVP) — `liyi check --json` provides rich stale-item context; an agent (using whatever model it already has) assesses each item and writes a structured report; `liyi triage --apply` acts on the report. The binary stays deterministic and offline; the LLM reasoning lives in the agentic workflow.
- **Agent instructions** — ~12 lines in AGENTS.md (plus a triage instruction for post-MVP).
- A **practice** — establish intent before (or alongside) execution.
- A **challenge mechanism** (post-MVP) — on-demand semantic verification of code against intent, or intent against requirement, driven by the agent.

## What This Is Not

- Not a CLI that wraps LLM calls. `liyi` never calls an LLM. The agent that's already running in the developer's workflow does the reasoning.
- Not a parser.
- Not a config system.
- Not a multi-week project.

---

## Adoption Story

### The problem

AI agents write most new code. Humans can't review it all. So they review less, and the code that does get reviewed is reviewed shallowly — rubber-stamping.

Intent is ephemeral. It lives in prompts, PR descriptions, Slack threads, context windows. When a different agent or human touches the code six months later, the intent is gone. The code is a fact; what it was *meant* to do is a memory.

Meanwhile, the same model writes the tests. Same training data, same blind spots. The tests confirm the implementation; they don't challenge it.

### The pitch

立意 makes intent persistent — written down, reviewed by humans, and durable across sessions and team changes. Once intent is persistent, a different AI can read it and try to break the code.

### Progressive adoption

Each level is independently valuable. Stop wherever the cost outweighs the benefit.

| Level | What you do | What you get | Human cost |
|---|---|---|---|
| **0. The instruction** | Add ~12 lines to AGENTS.md | Agent writes `.liyi.jsonc` alongside code. You have a persistent record of what each item is meant to do. | 15 minutes |
| **1. The review** | Review inferred intent in PRs — set `"reviewed": true` in sidecar (quick) or add `@liyi:intent` in source (explicit) | Reviewing 5 lines of intent is faster than reviewing 50 lines of implementation. You catch wrong intent before wrong code gets tested. Careless review undermines adversarial testing quality — see *Why careless review is self-limiting* in the Security Model. | Seconds per item |
| **2. The docs** | Add `## 立意` sections to READMEs / doc comments | Module-level invariants are documented, visible in rendered docs, discoverable by agents and humans. This is just good documentation practice. | 5 min per module |
| **3. The linter** | Run `liyi check` in CI | Stale specs fail the build. You know which items changed since their intent was written. Deterministic enforcement. | Install a binary |
| **3.5. Triage** | When stale items are flagged, the agent assesses each: cosmetic, semantic, or intent violation. `liyi triage --apply` auto-reanchors cosmetics. | Noise from refactors and renames is eliminated automatically. Remaining items are sorted by action type — update intent, fix code, or manual review. Graph-aware impact propagation flags transitively affected items. | Agent follows the triage instruction |
| **4. Challenge** | Click "Challenge" on a specced item in the editor, or include challenge in the agent workflow | A second model verifies code against intent, or intent against requirement. On-demand semantic verification — no pipeline, no test files. The trust gap between reviewing intent and trusting it blindly closes. | One click / prompt per item |
| **5. Requirements** | Write `@liyi:requirement` blocks and `@liyi:related` annotations for critical-path items | Requirements are tracked, hashable, versionable. When a requirement changes, all related items are transitively flagged. Challenge verifies intent actually covers the requirement, not just that hashes match. | Minutes per requirement |
| **6. The adversarial tests** | Configure a different model for test generation from reviewed specs | A second model reads the *intent* (not the code) and tries to break the implementation. Different training data, different blind spots. | Agent configuration |

### Why this and not X

- **vs. just writing good tests:** The same model wrote the code and the tests. 立意 splits the responsibility — one model states intent, a human reviews it, a different model attacks it.
- **vs. prose requirements + periodic LLM check ("intent watchdog"):** A team can write intent as prose and set up a periodic trigger that asks an LLM "is the codebase still implementing this requirement?" This covers ~60–70% of the stated value. What it lacks: *addressability* (prose requirements are unaddressed blobs — no structural link from a sentence to a specific function, specific lines, specific test), *incrementality* (the LLM re-evaluates the entire codebase every run — O(codebase) not O(changed items)), *trust stratification* (no distinction between agent-inferred and human-reviewed intent), *graph propagation* (no `related` edges, so transitive impact is invisible), and *ratchet enforcement* (no CI gate, so coverage degrades silently). 立意 is the indexing structure that makes LLM-based intent reasoning tractable at scale — hashes answer "what changed?" cheaply and deterministically; the LLM answers "does it matter?" expensively but only on stale items; the graph answers "who else should care?"
- **vs. AGENTS.md alone:** Agent instructions are probabilistic — compliance varies and can’t be verified. The linter is deterministic. Instructions tell agents what to do; the linter catches stale specs.
- **vs. formal contracts (Design by Contract, refinement types):** Those require learning a specification language and significant ramp-up cost. 立意 specs are natural language — the same language agents and humans already use. Strictly less powerful, but the cost to adopt is near zero.
- **vs. ADRs (Architecture Decision Records):** ADRs capture *why a decision was made* at a point in time. 立意 captures *what code should do right now*. ADRs are append-only history; 立意 specs are living artifacts that go stale when code changes. Complementary, not competing — ADRs explain the decision to adopt a module's design; `@liyi:module` captures the invariants that resulted.
- **vs. docstrings / JSDoc / rustdoc:** Docstrings describe *what code does* for human readers. 立意 specs describe *what code should do* for adversarial testing. Docstrings aren't tracked for staleness, aren't reviewed as a separate artifact, and aren't fed to a second model for attack. A docstring and a 立意 spec may contain similar text, but they serve different workflows and different consumers.
- **vs. Cursor rules / Windsurf rules / `.github/copilot-instructions.md`:** These are agent instructions — they tell the agent *how to behave* during generation. 立意's agent instruction is one of these (it lives in AGENTS.md). But the instruction alone is probabilistic; the linter and the sidecar convention are what make the output deterministic and durable. Rules files are the input; 立意 specs are the output.
- **vs. Augment Code's Intent** (launched Feb 2026): A proprietary desktop workspace for "spec-driven development" with multi-agent orchestration (Coordinator → Specialist → Verifier) and "living specs" that auto-update as agents work. Validates the thesis that specs should drive development, but differs fundamentally: *product vs. convention*, *optimistic vs. pessimistic*, *session-scoped vs. repo-durable*, *closed vs. open*. Intent's living specs auto-update (trusting the agent to keep specs accurate); 立意's staleness model flags when code changed and intent *didn't* (trusting the human to verify). Intent specs live inside Augment's workspace; 立意 specs are plain files in the repo that survive tool churn. Intent has no standalone CI linter; 立意's `liyi check` runs in any pipeline. Intent's verifier checks implementation against spec; 立意's adversarial testing uses a *different model* to attack the implementation based on *intent* (not code). Intent is a walled garden with BYOA flexibility; 立意 is a convention any tool can produce and consume.
- **vs. GitHub Spec Kit** (Sep 2025, MIT): Open-source, agent-agnostic templates for a Spec → Plan → Tasks → Implement workflow. Closest open-source competitor. But Spec Kit is *prescriptive only* — you write the spec first, agents implement it. No descriptive direction (inferring intent from existing code), no staleness tracking, no `source_hash`, no CI linter, no adversarial testing. It's a workflow template, not a convention with enforcement.
- **vs. AWS Kiro** (2025): Code OSS-based IDE with EARS notation specs, a primary agent with hooks automation, running on Amazon Bedrock. Single-agent, infrastructure-integrated, AWS-native. No file-based spec persistence outside the IDE; no vendor-neutral convention.

The spec-driven development space is no longer hypothetical — Augment Intent, GitHub Spec Kit, and Kiro are all shipping. No existing tool occupies 立意's exact niche: an open, vendor-neutral, file-based convention for AI-inferred intent specs, human-reviewed, with a deterministic CI linter and an adversarial testing pipeline. The closest analogue is GitHub Spec Kit (open, agent-agnostic), but it lacks enforcement, staleness tracking, and the descriptive direction. The closest in thesis is Augment Intent (spec-driven, verification), but it's a proprietary product, not an open convention.

### Selling points

- **Persistent by design.** Intent survives context windows, agent sessions, and team turnover. It's a file in the repo, not a message in a thread.
- **Each level stands alone.** You can adopt the instruction without the linter, or the linter without adversarial tests.
- **Nothing to learn.** JSONC, Markdown, SHA-256. No DSL, no specification language, no framework.
- **Lightweight.** The linter is ~500–800 lines of Rust with 4 direct dependencies. Small enough to audit, understand, and port to another language if needed.
- **No lock-in.** `.liyi.jsonc` files are plain JSONC. `@liyi:module` markers are comments. Delete them and nothing breaks.
- **Any programming language.** The linter doesn't parse source code. It reads line ranges from `source_span`, hashes them, compares. `.liyi.jsonc` is JSONC. `@liyi:module` markers use whatever comment syntax the host format already provides. Works with any language, any framework, any build system, any design pattern.
- **Any human language.** Intent prose is natural language — write it in your team’s working language. Annotation markers accept aliases in any supported language (`@liyi:ignore` / `@立意:忽略` / `@liyi:ignorar`). No locale configuration; the linter accepts all aliases from a static table. The project’s Chinese cultural origin isn’t a barrier — it’s an invitation.

### Who this is for

- Teams where AI writes a significant fraction of new code.
- Teams where code review is the bottleneck (or is being skipped).
- Solo developers who use AI agents and can't hold all intent in their head across weeks and months.
- Teams that want adversarial testing without building a framework.
- Any domain with testable correctness requirements — not just web services.
- Polyglot projects where the same business rule is implemented in multiple languages — one requirement, N implementations, shared traceability.

### Who this is not for

- Developers who read and remember all their own code (and believe they always will).
- Teams not using AI agents for code generation.
- Projects with formal verification or contracts already in place.
- Domains where the primary failure mode is performance or perceptual (rendering, audio, latency-critical hot paths) — NL specs can express constraints, but adversarial tests can't see pixels or measure nanoseconds.

### Intended workflow

What the day-to-day experience looks like once all deliverables exist:

1. **Write code.** (Or have an agent write it.) The agent instruction in AGENTS.md tells it to also generate `.liyi.jsonc` specs alongside the code.
2. **Review intent, not implementation.** The agent infers intent and writes the sidecar. Read the inferred intent (via IDE hover or in the sidecar diff). If correct, accept it — either set `"reviewed": true` (one click, zero source noise) or add `@liyi:intent=doc` in source (one line, maximum visibility). If wrong, correct the intent: write `@liyi:intent <prose>` in source with your own words, or edit the docstring. Reviewing 5 lines of intent is faster than reviewing 50 lines of implementation.
3. **CI runs `liyi check`.** The linter verifies that existing specs aren't stale (source hash matches) and reports unreviewed specs. Stale specs fail the build.
4. **Triage (optional).** When stale items are flagged, the agent assesses each: cosmetic, semantic, or intent violation. Cosmetics are auto-reanchored. Semantic changes get suggested intent updates. Violations are flagged for human review. The agent writes `.liyi/triage.json`; `liyi triage --apply` acts on it. This eliminates noise from refactors and focuses human attention on items that actually need re-review.
5. **Adversarial testing (optional).** A different model reads the reviewed intents and generates tests designed to break the implementation. Different training data, different blind spots.
6. **Iterate.** When source changes, the hash mismatches, the spec is flagged stale, the agent triages or re-infers, the human re-reviews. The cycle is fast because reviewing intent is fast.

**Steady state.** A team in steady state has reviewed intents for its critical-path items (not all items — trivial getters and simple wrappers use `@liyi:ignore`). The linter runs in CI and catches stale specs before they rot. Intent survives turnover: a new team member or agent reads the `.liyi.jsonc` files and understands what the code is *meant* to do, without reading every implementation.

### Migrating an existing project

No migration tool is needed. The agent instruction *is* the migration script.

Point an agent at the project with the 立意 instruction in its context and ask it to apply the convention, directory by directory. The agent reads each source file, infers intent for non-trivial items, writes `.liyi.jsonc` sidecar files, and adds `@liyi:module` blocks to existing READMEs or doc comments (or creates `LIYI.md` where none exist).

Practical notes:

- **Work directory by directory**, not whole-project at once. Context windows have limits; chunking by directory keeps inference quality high.
- **Everything starts unreviewed.** Bulk-inferred intents will be lower quality than intents written alongside code. That’s fine — no `"reviewed": true` and no `@liyi:intent` means the human hasn’t vouched for them yet. Review them at your own pace, or on first touch (set `"reviewed": true` via `liyi review`, or add `@liyi:intent=doc`).
- **The linter is immediately useful after bootstrap.** `liyi check` shows the full inventory: how many specs exist, how many are reviewed, how many are stale. It turns a vague "we should document intent" into a concrete progress tracker.
- **No special bootstrap command.** The same instruction that governs day-to-day development also handles cold start. One convention, one workflow.

### Success criteria

立意 succeeds as a project if it changes how teams think about AI-generated code. The measure is impact on the software engineering landscape, not internal completeness.

- **Dogfooding.** The 立意 project uses its own convention. The linter's source has `.liyi.jsonc` specs and `@liyi:module` markers. This is both principled and practical — it's the first demo.
- **Adoption.** Teams outside this project adopt the convention and report it useful. Even a handful of real-world users matters more than stars or downloads.
- **Validation.** At least one team reports catching a real defect through adversarial testing against reviewed specs that same-model testing missed. Until then, the adversarial hypothesis is plausible but unvalidated — validating it is a goal, not a claim.
- **Influence.** The practice of persisting AI-inferred intent gains traction, whether through this project's tooling or through the idea spreading independently. If someone builds a better version of 立意, that's still success.

On adversarial testing specifically: AI generates code faster than humans can review it, and hallucination is empirically infeasible to eliminate entirely. Any structured effort to contain that — even one strictly weaker than what formal verification or type-theoretic solutions offer — is worth the near-zero adoption cost.

---

## Risks and Open Questions

### 1. Positioning gap

The design doc is 1400 lines of careful reasoning. The deliverable is a JSONC sidecar convention and a 250-line linter. The intellectual depth is real, but the surface artifact is simple enough that it risks being dismissed as "just a staleness checker for doc comments." The pitch must convey why persistent intent matters *without* requiring the reader to follow the full design evolution from verification language (v3) through proc macros (v4) through interchange format (v5) through adversarial CLI (v6) to the current convention.

The progressive adoption ladder already does this structurally — each level has a clear "what you do / what you get" — but the one-paragraph pitch ("AI writes code, you can't read it all, 立意 makes intent persistent and checkable") needs to land on first contact. The project competes for attention against tools with flashier architecture and larger scope. Simplicity is the design's strength; it must also be the pitch's strength, not its liability.

### 2. Platform competition (updated 2026-03-06)

This risk is no longer speculative. Three concrete incumbents now occupy overlapping territory:

| Entrant | Nature | Launch | Key difference from 立意 |
|---|---|---|---|
| **Augment Code Intent** | Proprietary desktop workspace, closed source | Public beta Feb 2026; latest release 0.2.18 (Mar 3, 2026) | Product vs. convention; optimistic living specs vs. pessimistic staleness; session-scoped vs. repo-durable |
| **GitHub Spec Kit** | Open source (MIT), templates + CLI | Sep 2025; v0.1.4 as of Feb 2026 | Prescriptive only; no staleness tracking, no CI linter, no adversarial testing |
| **AWS Kiro** | Code OSS-based IDE, Amazon Bedrock | 2025 | EARS notation; single-agent; AWS-native; no vendor-neutral convention |

**Augment Intent is the most significant.** They chose the word "Intent" as their product name. Their thesis — specs should drive development, agents should be coordinated around specs, a verifier should check results — overlaps substantially with 立意's. They have $227M+ in funding, a full-time team, enterprise compliance (SOC 2 Type II, ISO/IEC 42001), and comparison pages already positioning against Kiro and Codex.

But Augment Intent is a **product** — a closed workspace you develop inside. 立意 is a **convention** — a way of organizing intent artifacts that any tool can produce and consume. The distinction is structural:

1. **Durability.** Augment's "living specs" auto-update *within a workspace session*. 立意 specs are files in the repo that survive sessions, agents, context windows, and team turnover. Stop using Augment and your specs are orphaned; delete 立意 files and nothing breaks.
2. **Trust model.** Augment's living specs are *optimistic* — the agent keeps them accurate automatically. 立意's staleness model is *pessimistic* — it flags when code changed and intent didn't, forcing a human decision. These are philosophically opposite: Augment trusts agents; 立意 trusts humans.
3. **CI enforcement.** Augment has no standalone linter. The verification happens inside their workspace. 立意's `liyi check` runs in any CI pipeline — GitHub Actions, GitLab CI, Jenkins — independent of any IDE or workspace product.
4. **Adversarial testing.** Augment's verifier checks that implementation matches spec (same direction as the coordinator). 立意's adversarial pipeline feeds reviewed intent to a *different model* that tries to *break* the implementation — different training data, different blind spots.
5. **Vendor neutrality.** `.liyi.jsonc` is plain JSONC. Any agent can write it; any tool can read it. Augment specs live inside Augment.

**The name collision is a discoverability problem.** Someone searching "intent-driven development" will find Augment's marketing. 立意's counter is its own name — the Chinese term, the `liyi` prefix, the `.liyi.jsonc` extension — none of which collide with "Intent" as a brand. The cultural identity is an asset, not a liability.

**GitHub Spec Kit validates the open-source approach** but doesn't compete on enforcement or durability. It's prescriptive only (spec → code), with no descriptive direction, no staleness tracking, and no CI linter. 立意 is strictly more capable.

**The urgency calculus has changed.** The previous version of this section said: "if a platform ships intent persistence first with a proprietary format, 立意 becomes an interoperability layer or dies." A platform *has* shipped. The window for establishing convention gravity is narrower than assumed. The "build effort: a few hours" estimate is now the strategic lifeline — ship the linter, use it on real projects, demonstrate that an open convention is more durable than a proprietary workspace.

**The thesis is validated, not threatened.** A well-funded company betting its product on spec-driven development confirms that the problem is real and the market is forming. If the spec-driven development wave settles on proprietary living specs inside vendor workspaces, 立意 is a footnote. If the community values vendor-neutral, file-based, CI-enforceable intent persistence — the way `.editorconfig` won over IDE-specific formatting settings — 立意 is the convention that fills that role. The outcome depends on shipping, not on design.

### 3. Adversarial testing hypothesis is unvalidated

The strongest differentiator — "a second model, reading reviewed intent, catches bugs the first model missed" — is explicitly a hypothesis, not a demonstrated capability. The design correctly treats it as level 6 (last, optional) and doesn't make it load-bearing. Every prior level delivers value independently.

But validation is necessary for the project to be more than a staleness checker. The success criterion is clear: at least one team reports catching a real defect through adversarial testing against reviewed specs that same-model testing missed. Until then, the claim is architectural reasoning ("different model, different blind spots"), not evidence.

**Action:** After shipping the linter, run the adversarial testing experiment on a real codebase (the project's own, or a volunteer's) and publish the results — including null results if the hypothesis doesn't hold. Honest reporting of a null result would still be a contribution to the field.

### 4. `source_span` brittleness in 0.1 (mitigated by `tree_path`)

Line-number-based spans mean that any edit changing line counts (adding an import, inserting a blank line) invalidates every spec whose `source_span` falls at or below the edit point. The span-shift heuristic (±100-line scan, delta propagation) handles uniform shifts — the most common case — and reports `SHIFTED` (auto-corrected) rather than `STALE`. `liyi reanchor` handles non-uniform shifts manually.

**v8.4 update:** This risk prompted the introduction of `tree_path` in 0.1 (see *Structural identity via `tree_path`*). When `tree_path` is populated, span recovery is deterministic — the tool locates the item by AST identity regardless of how lines shifted. The span-shift heuristic remains as a fallback for items without a `tree_path` (macros, generated code, unsupported languages).

The remaining friction for items without `tree_path`: between agent sessions, manual edits that shift lines without an agent re-inference will produce CI noise until the developer runs `liyi reanchor`. This is the same class of friction as lockfile conflicts (run `pnpm install` after merge), but it's friction nonetheless. For supported languages (Rust in 0.1), `tree_path` eliminates this friction entirely.

**Mitigation in 0.1:** `tree_path` structural anchoring (primary), span-shift auto-correction (fallback), `liyi reanchor`, agent re-inference on next pass.

### 5. Convention absorption and licensing (added 2026-03-06)

A well-funded competitor (Augment Code, with their Intent product) can absorb the 立意 convention into a proprietary offering without contributing back. The absorption path is straightforward:

1. **Index `.liyi.jsonc` as context.** Their Context Engine already indexes arbitrary project files. Adding `.liyi.jsonc` awareness improves their agents' context quality with structured intent data from any repo that adopts 立意. One-way value extraction.
2. **Reimplement the staleness model.** If their "living specs" prove unreliable (auto-updating specs drift silently), `source_hash` + `source_span` staleness is a public algorithm, fully specified in this document, trivially reimplementable. They ship "staleness alerts" as a feature.
3. **Ship `.liyi.jsonc` import/export.** If the convention gains traction, they offer compatibility as a feature — their specs are primary, `.liyi.jsonc` is a second-class interop format. They absorb the convention's ecosystem without contributing to it.

**No license can prevent this.** The convention is a file format (`.liyi.jsonc`), a set of marker strings (`@liyi:module`, `@liyi:intent`), and a staleness algorithm (hash lines, compare). These are ideas and data formats — not copyrightable expression. Even under AGPL, a competitor reimplements the algorithm from this public specification without touching the linter's source code. The JSON Schema is a functional specification. The linter is 500–800 lines of straightforward Rust — reimplementation cost is one engineer-day.

Copyleft (GPL, AGPL, MPL) would protect the **linter binary** from being embedded in a closed product without releasing source. But:

- Augment wouldn't fork the linter; they'd reimplement it.
- Copyleft on a convention chills enterprise adoption — the exact audience that would validate the thesis.
- No convention or protocol in widespread use is copyleft: `.editorconfig` (MIT), semver (CC-BY-3.0), Conventional Commits (CC-BY-3.0), JSON Schema (Apache-2.0/MIT).

**Licensing decision: Apache-2.0 OR MIT** (dual license, user's choice).

- **Rust ecosystem convention.** The Rust project, serde, tokio, clap, and nearly every crate on crates.io use `Apache-2.0 OR MIT`. The linter is a Rust binary; following the ecosystem convention removes friction for Rust contributors and downstream packagers.
- **Patent grant via Apache-2.0.** Apache-2.0 includes an explicit patent license, which MIT does not. Adopters who care about patent protection pick the Apache-2.0 side.
- **Simplicity via MIT.** Some organizations have pre-approved MIT but haven't reviewed Apache-2.0. Some jurisdictions (notably parts of the CJK world, where early adopters are likely) have institutional workflows where MIT's brevity is an advantage.
- **No GPL-incompatibility trap.** Apache-2.0 is incompatible with GPL-2.0-only. The MIT side covers anyone integrating into a GPL-2.0-only project.
- **Convention gravity over exclusion.** Conventions win by adoption. Permissive licensing removes friction for the enterprise teams, polyglot shops, and platform integrators who are the primary adoption targets.
- **Cultural attribution is the durable moat.** 立意 is a Chinese cultural concept. The `.liyi.jsonc` extension, the `@liyi:module` markers, the name, the design document's reasoning — these carry attribution that no license provides and no fork erases. If Augment ships "立意 compatibility," they're advertising the convention by name.
- **Ecosystem gravity is the real defense.** If `liyi check` becomes the standard CI linter for intent specs — the way `rustfmt` is the standard formatter, the way `.editorconfig` is the standard config — competitors will interoperate with it rather than replace it. The tool is simple enough that reimplementation is pointless when the original works everywhere.

**This is a deliberate trade.** The designer accepts that Augment (or anyone) can reimplement the convention, absorb it into a proprietary product, and monetize it without contributing back. By the project's own success criteria — "the practice of persisting AI-inferred intent gains traction, whether through this project's tooling or through the idea spreading independently" — that scenario is a form of success. The convention spreading inside a walled garden is less good than the convention spreading openly, but better than the convention not spreading at all.

The risk that absorption *prevents* the open convention from thriving — by pulling potential adopters into a proprietary implementation — is real but mitigated by the same dynamics that keep `.editorconfig` alive despite every IDE having its own formatting settings: the open tool is simpler, works everywhere, and has no vendor lock-in. The bet is that simplicity and universality outweigh product polish.

---

## Appendix: Worked Example

A complete cycle — source, sidecar, linter output, code change, staleness, re-review — for one function.

### 1. Source

```rust
// src/billing/money.rs

// @liyi:requirement multi-currency-addition
// Add two monetary amounts of the same currency.
// Must reject mismatched currencies with an error, not a panic.
// Must be commutative. Must not overflow silently.

// @liyi:related multi-currency-addition
pub fn add_money(a: Money, b: Money) -> Result<Money, CurrencyError> {
    if a.currency != b.currency {
        return Err(CurrencyError::Mismatch(a.currency, b.currency));
    }
    Ok(Money { cents: a.cents + b.cents, currency: a.currency })
}
```

### 2. Sidecar (after agent + tool)

The agent writes `source_span` and `intent`. The tool (`liyi reanchor` or the linter on first run) fills in `source_hash` and `source_anchor`. The result on disk:

```jsonc
// src/billing/money.rs.liyi.jsonc
{
  "version": "0.1",
  "source": "src/billing/money.rs",
  "specs": [
    {
      "requirement": "multi-currency-addition",
      "source_span": [3, 6],
      "source_hash": "sha256:b7c8d9...",
      "source_anchor": "// @liyi:requirement multi-currency-addition"
    },
    {
      "item": "add_money",
      "intent": "Add two monetary amounts. Reject mismatched currencies with Err, not panic. Commutative. No silent overflow.",
      "related": {
        "multi-currency-addition": "sha256:b7c8d9..."
      },
      "source_span": [9, 14],
      "source_hash": "sha256:a1b2c3...",
      "source_anchor": "pub fn add_money(a: Money, b: Money) -> Result<Money, CurrencyError> {"
    }
  ]
}
```

### 3. Linter output (before review)

```
$ liyi check src/billing/

multi-currency-addition: ✓ requirement, tracked
add_money: ⚠ unreviewed (no @liyi:intent, no reviewed:true)
```

### 4. Human reviews

The human reads the inferred intent (in the sidecar or via IDE hover) and confirms it. Two paths:

**Quick approval** — set `"reviewed": true` in the sidecar (via `liyi review src/billing/money.rs add_money` or IDE code action). Zero source noise:

```
$ liyi check src/billing/

multi-currency-addition: ✓ requirement, tracked
add_money: ✓ reviewed, current
```

**Explicit override** — add `@liyi:intent` in source when you want maximum visibility or want to state intent in your own words:

```rust
// @liyi:intent Add two monetary amounts. Reject mismatched currencies
//   with Err, not panic. Commutative. No silent overflow.
// @liyi:related multi-currency-addition
pub fn add_money(a: Money, b: Money) -> Result<Money, CurrencyError> {
```

```
$ liyi check src/billing/

multi-currency-addition: ✓ requirement, tracked
add_money: ✓ reviewed, current
```

Both paths produce the same linter output. Source intent takes precedence for adversarial testing when present.

### 5. Code changes (someone adds overflow checking)

```rust
pub fn add_money(a: Money, b: Money) -> Result<Money, CurrencyError> {
    if a.currency != b.currency {
        return Err(CurrencyError::Mismatch(a.currency, b.currency));
    }
    let cents = a.cents.checked_add(b.cents)
        .ok_or(CurrencyError::Overflow)?;
    Ok(Money { cents, currency: a.currency })
}
```

```
$ liyi check src/billing/

multi-currency-addition: ✓ requirement, tracked
add_money: ⚠ STALE — source changed since spec was written
```

The agent re-infers (updating `source_span`; the tool recomputes `source_hash`), the human re-reviews. Cycle complete.

---

---

## Appendix: JSON Schema for `.liyi.jsonc` (v0.1)

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://liyi.run/schema/0.1/liyi.schema.json",
  "title": "立意 sidecar spec file",
  "type": "object",
  "required": ["version", "source", "specs"],
  "additionalProperties": false,
  "properties": {
    "version": {
      "type": "string",
      "const": "0.1",
      "description": "Schema version. The linter rejects unknown versions."
    },
    "source": {
      "type": "string",
      "description": "Path to the source file, relative to the repository root."
    },
    "specs": {
      "type": "array",
      "items": {
        "oneOf": [
          { "$ref": "#/$defs/itemSpec" },
          { "$ref": "#/$defs/requirementSpec" }
        ]
      }
    }
  },
  "$defs": {
    "sourceSpan": {
      "type": "array",
      "items": { "type": "integer", "minimum": 1 },
      "minItems": 2,
      "maxItems": 2,
      "description": "Closed interval of 1-indexed line numbers [start, end]. start must be <= end."
    },
    "sourceHash": {
      "type": "string",
      "pattern": "^sha256:[0-9a-f]+$",
      "description": "SHA-256 hex digest of the source lines in the span, prefixed with 'sha256:'."
    },
    "itemSpec": {
      "type": "object",
      "required": ["item", "intent", "source_span"],
      "additionalProperties": false,
      "properties": {
        "item": {
          "type": "string",
          "description": "Display name of the item (function, struct, macro, etc.). Not a unique key — identity is item + source_span."
        },
        "reviewed": {
          "type": "boolean",
          "default": false,
          "description": "Optional. Whether a human has reviewed and accepted this intent via sidecar approval. Defaults to false when absent. The linter also considers an item reviewed if @liyi:intent is present in source."
        },
        "intent": {
          "type": "string",
          "description": "Natural-language description of what the item SHOULD do, or the sentinel value '=doc' meaning the source docstring captures intent."
        },
        "source_span": { "$ref": "#/$defs/sourceSpan" },
        "tree_path": {
          "type": "string",
          "default": "",
          "description": "Optional. Structural AST path for tree-sitter-based span recovery (e.g., 'fn::add_money', 'impl::Money::fn::new'). When non-empty, the tool uses tree-sitter to locate the item by structural identity. When empty or absent, falls back to line-number-based span matching. Tool-managed — agents MAY write this but the tool overwrites with the canonical form."
        },
        "source_hash": {
          "$ref": "#/$defs/sourceHash",
          "description": "Tool-managed. SHA-256 hex digest of the source lines in the span. Computed by liyi reanchor or the linter — agents should not produce this."
        },
        "source_anchor": {
          "type": "string",
          "description": "Literal text of the first line of the span. Tool-managed — agents should not produce this."
        },
        "confidence": {
          "type": "number",
          "minimum": 0,
          "maximum": 1,
          "description": "Optional. Agent's self-assessed confidence in the inferred intent. May be removed after review."
        },
        "related": {
          "type": "object",
          "additionalProperties": {
            "oneOf": [
              { "$ref": "#/$defs/sourceHash" },
              { "type": "null" }
            ]
          },
          "description": "Optional. Maps requirement names to their source_hash at time of last review. Agents write null; the tool fills in hashes."
        }
      }
    },
    "requirementSpec": {
      "type": "object",
      "required": ["requirement", "source_span"],
      "additionalProperties": false,
      "properties": {
        "requirement": {
          "type": "string",
          "description": "Name of the requirement. Unique per repository."
        },
        "source_span": { "$ref": "#/$defs/sourceSpan" },
        "tree_path": {
          "type": "string",
          "default": "",
          "description": "Optional. Structural AST path for tree-sitter-based span recovery. When non-empty, the tool uses tree-sitter to locate the requirement by structural identity. When empty or absent, falls back to line-number-based span matching. Tool-managed."
        },
        "source_hash": {
          "$ref": "#/$defs/sourceHash",
          "description": "Tool-managed. Computed by liyi reanchor or the linter."
        },
        "source_anchor": {
          "type": "string",
          "description": "Literal text of the first line of the span. Tool-managed."
        }
      }
    }
  }
}
```

This schema ships as `liyi.schema.json` in the linter's release artifacts and is published at the `$id` URL. Editors that support JSON Schema (VSCode, IntelliJ, Neovim with `SchemaStore`) will provide validation and autocompletion for `.liyi.jsonc` files when configured with `"$schema": "https://liyi.run/schema/0.1/liyi.schema.json"` at the top of the sidecar, or via a workspace-level `json.schemas` setting.

---

## AIGC Disclaimer

This document contains content from the following AI agents:

* Claude Opus 4.6
* Claude Sonnet 4.6
* DeepSeek
* GPT-5.2
* Kimi K2.5 Instant
* Kimi K2.5 Thinking

The document is primarily authored by Claude Opus 4.6, with the human designer's input, and multiple rounds of adversarial review.
