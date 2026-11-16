<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
<!-- AIGC: drafted with AI assistance (Claude Opus 4.8 via OpenCode); reviewed by a human maintainer before merge. -->

# `@liyi:file` File-Scoped Directives

**Status:** 🔵 Proposed (pending implementation)
**Target:** v0.2
**Design authority:** this document
**Related:** `docs/liyi-design.md` — *Multilingual annotations*, *File-level: `.liyiignore`*, *Self-hosting and the quine problem*; `docs/injection-impl.md` (language detection consumers)

---

## Motivation

立意 already has file-granularity controls in different places, covering
different needs:

- `.liyiignore` excludes whole files from discovery, gitignore-style, from an
  out-of-band sidecar file. It works for files you cannot annotate (generated
  code, vendored deps).
- Language detection is inferred purely from the file extension
  (`detect_language` in `crates/liyi/src/tree_path/mod.rs`). There is no way for
  a file to declare "parse me as X" when the extension is ambiguous, absent, or
  misleading.

These gaps motivate a unified, *in-file* directive:

1. **Extension ambiguity.** `.h` is claimed by the C++ grammar, but a given
   header may be plain C; a `.txt`, a `.in` template, or an extensionless script
   carries no usable extension signal at all. The owning file is the right place
   to resolve this — the author knows the dialect.
2. **Owner-driven ignore.** `.liyiignore` is the right tool for files you *don't*
   own, but for a file you *do* own, adding a separate `.liyiignore` entry is
   indirection. A one-line marker at the top of the file, next to the code it
   governs, is more local and survives file moves.

`@liyi:file` introduces a single namespaced directive family for file-scoped
metadata, parallel to the existing item-scoped (`@liyi:ignore`, `@liyi:intent`)
and module-scoped (`@liyi:note`, see `docs/note-context-design.md`) marker
families.

---

## Surface syntax

A `@liyi:file` directive is a marker followed by one space-separated sub-key,
optionally with a `key=value` argument. It is written in the host language's
comment syntax, exactly like every other 立意 marker, and is subject to the
same full-width normalization rules; its keyword is canonical ASCII (there are
no localized keyword aliases).

```rust
// @liyi:file language=c
// @liyi:file ignore
```

```python
# @liyi:file language=python
```

```yaml
# @liyi:file ignore
```

Directives are recognized anywhere a marker scan reaches, but the **conventional
placement is the file header** (first comment block). Multiple `@liyi:file`
directives may appear in one file; they accumulate (e.g. a `language=` and an
`ignore` together).

### Sub-keys (v0.2)

| Sub-key | Argument | Effect |
|---|---|---|
| `language` | `=<lang>` (required) | Override grammar/language detection for this file. |
| `ignore` | none | Exclude this file from discovery, additively with `.liyiignore`. |

The namespace is intentionally open: future file-scoped metadata (e.g.
`@liyi:file dialect=...`, `@liyi:file generated`) slots in without a new marker
keyword.

---

## Normative requirements

<!-- @liyi:requirement file-directive-namespace -->
**`@liyi:file` is a single namespaced directive family, not a set of distinct
markers.** The scanner recognizes one canonical marker keyword
(`@liyi:file`) followed by a sub-key token. Sub-keys are an open set;
encountering an unknown sub-key is a recoverable diagnostic (warning), never a
hard parse error, so that a file authored against a newer 立意 version degrades
gracefully on an older binary. The sub-key grammar is `<sub-key>` or
`<sub-key>=<value>`, with at most one `=` argument per directive; whitespace
around the directive is insignificant. This mirrors the item-level annotation
family (`@liyi:ignore`, `@liyi:trivial`) rather than minting a marker per
concern.
<!-- @liyi:end-requirement file-directive-namespace -->

<!-- @liyi:requirement file-language-precedence -->
**Inline `@liyi:file language=` overrides all other language detection, with a
defined precedence order.** Language resolution for a file proceeds in strict
priority: (1) an inline `@liyi:file language=<lang>` directive in the file;
(2) an injection profile or other out-of-band configuration that names the
file's language; (3) extension-based detection. The first source that yields a
known language wins. An inline directive naming an *unknown* language is an
error (the author asserted something the tool cannot honor), not a silent
fallthrough to extension detection — surfacing the typo is more useful than
guessing. This makes the file's own declaration authoritative, because the
author has more context than the extension table (e.g. a `.h` that is C, not
C++).
<!-- @liyi:end-requirement file-language-precedence -->

<!-- @liyi:requirement file-ignore-additive -->
**`@liyi:file ignore` is additive to `.liyiignore` and distinct from
`@liyi:ignore`.** A file bearing `@liyi:file ignore` is excluded from discovery
exactly as if a matching pattern existed in a `.liyiignore` — no specs are
required, no diagnostics are emitted for it. This is a *file-level* exclusion
and must not be confused with the *item-level* `@liyi:ignore`, which opts a
single item out of speccing while the rest of the file is still processed.
The two mechanisms compose: a file can be discovered with some items
`@liyi:ignore`-d, or excluded wholesale with `@liyi:file ignore`. The directive
never *re-includes* a file excluded by `.liyiignore`; it only adds exclusions
(there is no in-file negation, because a file cannot un-ignore itself once the
scanner has been told to skip it). Ownership separation is preserved:
`.liyiignore` for files you don't own, `@liyi:file ignore` for files you do.
<!-- @liyi:end-requirement file-ignore-additive -->

---

## Interaction with existing mechanisms

**vs. `.liyiignore`.** Both exclude files; they differ in locus of control.
`.liyiignore` is directory-scoped, gitignore-cascading, and external — correct
for generated/vendored files. `@liyi:file ignore` is in-file and travels with
the file. When both apply, the file is excluded (exclusions union; there is no
inline re-inclusion).

**vs. item-level `@liyi:ignore`.** Disjoint granularity. `@liyi:file ignore`
removes the whole file from discovery; `@liyi:ignore` removes one item from
speccing within a discovered file. The shared word "ignore" is deliberate (one
concept, two scopes) but the `file` sub-key makes the scope explicit.

**vs. injection profiles / extension detection.** `@liyi:file language=` sits at
the top of the precedence chain (see `file-language-precedence`). Injection
profiles (`docs/injection-impl.md`) remain authoritative for *embedded* dialects
within a host file; `@liyi:file language=` sets the *host* language and does not
describe sub-spans.

**vs. marker keywords.** `@liyi:file` joins the canonical marker keyword list
(`crates/liyi/src/markers.rs`) on the same terms as other markers. Its keyword
is canonical ASCII; sub-key *values* such as language names are stable tokens
(like a requirement name), not localized.

---

## Implementation surface (non-normative)

- `crates/liyi/src/markers.rs`: add a `File { sub_key, value, line }` variant to
  `SourceMarker`, extend the alias table with `@liyi:file` (escaped `\x40` per
  *quine-escape-in-source*), and parse the `<sub-key>[=<value>]` argument.
- `crates/liyi/src/tree_path/mod.rs`: introduce
  `detect_language_with_meta(path, content)` that consults inline directives
  before falling back to `detect_language(path)`. Fix the stale doc comment that
  claims `.h` is C while `lang_cpp` claims the `h` extension.
- `crates/liyi/src/discovery.rs`: consult `@liyi:file ignore` alongside the
  `.liyiignore` cascade, unioning exclusions.
- Diagnostics: unknown sub-key → warning; `language=<unknown>` → error.

The implementation plan and its requirement coverage will be tracked when the
work is scheduled (see `docs/next-steps.md`).
