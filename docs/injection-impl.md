<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Language Injection Framework: Implementation Plan

**Status:** ✅ Complete (core framework + GitHub Actions profile)
**Target:** v0.1.x
**Design authority:** `docs/liyi-design.md` — *Quoting and injection*, *Appendix: tree_path Grammar Specification (v0.3)*

---

## Motivation

The current `LanguageConfig` architecture assumes one grammar per file. Several
important file types violate this: GitHub Actions YAML embeds shell in `run:`
blocks, Vue SFCs embed TypeScript and CSS, HTML embeds JavaScript and CSS.

The data-file work (TOML, JSON, YAML) brought config files under structural
identity, but YAML `key.jobs::key.build::key.steps[1]::key.run` is a
terminal node — the Bash code inside the string is invisible to the resolver.
The injection framework makes it visible.

---

## Design constraints

The following constraints are normative for the implementation.

<!-- @liyi:requirement injection-profile-isolation -->
**Injection profiles are isolated from grammar configs.** `LanguageConfig`
defines grammar mechanics (kind maps, body fields, transparent kinds). Injection
profiles define *where embedded code lives* and *what language it is* — a
separate concern. Adding a new injection profile (e.g., GitLab CI) must not
modify `lang_yaml.rs` or any existing `LanguageConfig`. This is the primary
extensibility invariant: one file per dialect, zero cross-file edits.
<!-- @liyi:end-requirement injection-profile-isolation -->

<!-- @liyi:requirement path-based-dialect-detection -->
**Path-based dialect detection is the primary heuristic.** Repo-relative file
paths (as stored in sidecar `"source"` fields) carry high-signal structural
information. `.github/workflows/**/*.yml` is GitHub Actions with near-zero false
positives. Path patterns are cheap, deterministic, and require no parsing. They
are evaluated before content-based heuristics.
<!-- @liyi:end-requirement path-based-dialect-detection -->

<!-- @liyi:requirement no-injection-without-profile -->
**No injection without a matching profile.** When no `InjectionProfile` matches
a file's path (and no content heuristic fires), the file is parsed with its base
grammar only — exactly the current behavior. Injection is strictly additive;
files without a matching profile are never degraded.
<!-- @liyi:end-requirement no-injection-without-profile -->

<!-- @liyi:requirement injection-pair-attachment -->
**The `//lang` marker attaches within a pair.** Injection markers attach to
the name within a pair (`run//bash`), not as standalone segments. This means
existing resolver, serializer, and sibling-scan logic continues to work on the
host portion of the path. The `Pair` struct carries an optional `injection`
field.
<!-- @liyi:end-requirement injection-pair-attachment -->

<!-- @liyi:requirement content-offset-correctness -->
**Span translation must account for content offset.** When sub-parsing injected
content, all line numbers from the inner parser are relative to the injected
string. These must be translated to outer-file line numbers by adding the host
node's start line. Off-by-one errors here silently corrupt `source_span` — the
translation must be covered by roundtrip tests.
<!-- @liyi:end-requirement content-offset-correctness -->

<!-- @liyi:requirement ancestor-path-matching -->
**Injection rules support ancestor-path matching.** A key name alone is
sometimes ambiguous — `run` in GitHub Actions vs. `run` in an unrelated YAML
schema. Rules may specify an ancestor key path that must be satisfied for the
injection to fire. This prevents false-positive injection in non-CI YAML files
that happen to match a profile's path pattern but use `run:` for a different
purpose.
<!-- @liyi:end-requirement ancestor-path-matching -->

---

## Scope

### In scope

- ✅ `InjectionProfile` struct — static, data-driven, one per dialect.
- ✅ `InjectionRule` struct — key name, injected language, ancestor constraint.
- ✅ Profile registry — static slice of `&InjectionProfile` refs.
- ✅ `detect_injection_profiles()` — path-pattern matching, returns matching
  profiles for a given repo-relative path.
- ✅ `resolve_tree_path` extension — when a `Segment::Injection` is encountered,
  switch parser and config, apply line offset, continue resolving.
- ✅ `compute_tree_path` extension — detect when the target node is inside an
  injection zone, emit `//lang` in the appropriate segment.
- ✅ Content extraction — extract the string value from a host node, strip YAML
  block-scalar indicators and indentation.
- ✅ Span translation — map inner-parser spans back to outer-file line numbers.
- ✅ GitHub Actions profile (P1): `.github/workflows/**/*.yml|yaml`, `run:` →
  Bash.
- ✅ Tests: roundtrip tests for composite paths, span translation tests, profile
  detection tests.

### Out of scope (deferred)

- Content-based dialect detection (sniffing for `jobs:` + `on:` keys).
- Vue SFC injection — depends on `tree-sitter-vue` maturation.
- HTML `<script>`/`<style>` injection.
- Jupyter notebook injection (JSON cells → Python).
- GitLab CI profile — straightforward to add once the framework exists, but
  not P1.
- YAML block-scalar edge cases (folded `>`, chomping indicators) — handle the
  common `|` literal block first.

---

## Architecture

### Module layout

```
tree_path/
  lang_yaml.rs              # LanguageConfig — unchanged
  lang_json.rs              # LanguageConfig — unchanged
  inject/
    mod.rs                   # InjectionProfile, InjectionRule, registry, detection
    github_actions.rs        # Profile: .github/workflows → run: → Bash
    gitlab_ci.rs             # Profile: .gitlab-ci.yml → script: → Shell (future)
```

Each dialect file defines a single `static PROFILE: InjectionProfile` and
nothing else. Adding a new dialect is: create the file, add a `mod` declaration,
add a `&PROFILE` reference to the registry slice.

`lang_yaml.rs` is never touched. The `LanguageConfig` and `InjectionProfile`
are orthogonal axes — grammar mechanics vs. dialect-specific embedding rules.

### Core types

```rust
/// A rule describing one injection point within a host grammar.
pub struct InjectionRule {
    /// Key name that triggers injection (e.g., "run").
    pub key_name: &'static str,
    /// Language to sub-parse the injected content as.
    pub language: Language,
    /// Optional ancestor key path that must be satisfied.
    /// Each element is a key name; the rule fires only if the
    /// injection-point node has these ancestors in order
    /// (not necessarily immediate parents).
    ///
    /// Example: &["jobs"] requires that the `run:` key appears
    /// somewhere under a `jobs:` key.
    pub ancestor_keys: &'static [&'static str],
}

/// An injection profile associates a host language + file-path
/// pattern with a set of injection rules.
pub struct InjectionProfile {
    /// Host language this profile applies to.
    pub host: Language,
    /// Glob patterns matched against the repo-relative file path.
    /// If any pattern matches, this profile is active.
    /// Empty slice = never auto-activated (explicit only).
    pub path_patterns: &'static [&'static str],
    /// Injection rules to apply when this profile is active.
    pub rules: &'static [InjectionRule],
}
```

### Example profile: GitHub Actions

```rust
// inject/github_actions.rs

pub(crate) static PROFILE: InjectionProfile = InjectionProfile {
    host: Language::Yaml,
    path_patterns: &[
        ".github/workflows/**/*.yml",
        ".github/workflows/**/*.yaml",
        ".github/actions/**/*.yml",
        ".github/actions/**/*.yaml",
    ],
    rules: &[
        InjectionRule {
            key_name: "run",
            language: Language::Bash,
            ancestor_keys: &["jobs"],
        },
    ],
};
```

### Profile detection

```rust
/// Returns all injection profiles whose path patterns match the given
/// repo-relative path.  Returns an empty slice when no profile matches
/// (= no injection, base grammar only).
pub fn detect_injection_profiles(path: &Path) -> Vec<&'static InjectionProfile> {
    REGISTRY.iter()
        .filter(|p| p.matches_path(path))
        .collect()
}
```

`matches_path` uses the same glob matching as `.liyiignore` (the `globset`
crate is already a dependency). Path matching operates on repo-relative paths
— the same paths stored in sidecar `"source"` fields.

### Resolver extension

When `resolve_tree_path` encounters a `Segment::Injection(lang)` after
resolving the host-side name segment:

1. **Find the host node** — the name segment immediately before `//lang`
   identifies the host node (e.g., the `run:` pair).
2. **Extract content** — get the string value of the host node. For YAML
   block scalars, strip the indicator line (`|`, `>`, `|+`, `|-`) and
   de-indent the body.
3. **Record the line offset** — the host node's value starts at some line
   in the outer file; this offset is added to all inner spans.
4. **Sub-parse** — parse the extracted content with the injected language's
   grammar.
5. **Continue resolving** — the remaining path segments after `//lang` are
   resolved against the inner parse tree using the injected language's
   `LanguageConfig`.
6. **Translate span** — the final span from the inner resolver is
   translated back to outer-file coordinates by adding the line offset.

### Compute extension

When `compute_tree_path` walks from root to target and finds that the target
node is a string literal whose parent is a key matched by an active injection
rule:

1. Build the host-side path up to the injection-point key.
2. Emit the `//lang` marker.
3. Sub-parse the string content, locate the target within the inner tree,
   and build the inner path.
4. Concatenate: `host_path//lang::inner_path`.

### Ancestor matching

To check `ancestor_keys`, walk from the injection-point node upward via
`node.parent()`, collecting key names. The rule fires only if every element
in `ancestor_keys` appears in the ancestor chain in order. This is an
ordered subsequence check, not an exact path match — intervening keys are
allowed.

Example for GitHub Actions `run:` with `ancestor_keys: &["jobs"]`:

```
document → block_mapping → jobs(pair) → ... → steps[1](pair) → run(pair)
                            ^^^^
                            ancestor "jobs" found → rule fires
```

If the same file had a top-level `run:` key outside `jobs:`, the ancestor
check would not find `"jobs"` in the ancestor chain, and the rule would
not fire.

---

## Content extraction

### YAML block scalars

The most common injection form in GitHub Actions is:

```yaml
run: |
  set -euo pipefail
  cargo build --release
```

The tree-sitter-yaml AST represents this as a `block_scalar` node whose text
includes the `|` indicator line. Extraction must:

1. Identify the `value` field of the `block_mapping_pair`.
2. If the value is a `block_scalar`, strip the first line (the indicator:
   `|`, `>`, `|+`, `|-`, `|2`, etc.).
3. Determine the base indentation from the first content line and strip it
   uniformly.
4. Record the line offset as the host node's value start line + 1 (skipping
   the indicator line).

### YAML flow scalars

Inline strings (`run: "echo hello"`) are `flow_node → double_quote_scalar`
or `plain_scalar`. These are typically single-expression commands — still
valid Bash, but less commonly multi-line. Extraction strips surrounding
quotes and unescapes YAML escape sequences.

---

## Extensibility path

### Adding a new dialect

1. Create `inject/<dialect>.rs` with a `static PROFILE: InjectionProfile`.
2. Add `mod <dialect>;` to `inject/mod.rs`.
3. Add `&<dialect>::PROFILE` to the `REGISTRY` slice.
4. Write tests in the new file's `#[cfg(test)]` module.

No changes to `lang_yaml.rs`, `mod.rs`, or any existing profile. The
three-step process is the same regardless of whether the dialect is another
YAML variant (GitLab CI, CircleCI, K8s) or a completely different host
language (HTML, Vue).

### Future: K8s awareness

Kubernetes manifests don't embed shell in the same way as CI files, but they
do have structured semantics. Two possible extensions:

- **Command injection:** `spec.containers[*].command` and `args` may contain
  shell when used with `/bin/sh -c`. An `InjectionRule` with
  `ancestor_keys: &["spec", "containers"]` and `key_name: "command"` could
  handle this, though the heuristic is weaker (not all `command:` arrays are
  shell).
- **Schema-aware kind mappings:** Kubernetes has well-known resource structures
  (Deployment, Service, ConfigMap). A future extension could provide richer
  `kind_map` entries that use the `apiVersion`/`kind` fields to select
  resource-specific vocabularies. This is beyond injection — it's dialect-aware
  grammar config — and would require a separate design pass.

### Future: non-YAML hosts

The `InjectionProfile` struct generalizes beyond YAML:

- **Vue SFC:** `host: Language::Html` (or a future `Language::Vue`),
  `key_name` replaced by a tag-name matcher for `<script>`, `<style>`.
  The rule shape may need extending — tag-based injection uses element names
  and attributes rather than key names.
- **HTML:** Same pattern as Vue but without the `lang` attribute detection.
- **Jupyter:** `host: Language::Json`, injection in `"source"` arrays within
  `"cell_type": "code"` objects. Complex enough to warrant its own rule shape
  (JSON path + content-type check).

The `InjectionRule` fields (`key_name`, `ancestor_keys`) are key-centric by
design, optimized for the YAML case. When non-key-based injection points
arise, the rule type should be extended with an enum:

```rust
enum InjectionMatch {
    /// Match by key name (YAML, JSON).
    KeyName {
        key_name: &'static str,
        ancestor_keys: &'static [&'static str],
    },
    /// Match by element/tag name (HTML, Vue).
    TagName {
        tag_name: &'static str,
        /// Optional attribute for language detection (e.g., `lang="ts"`).
        lang_attr: Option<&'static str>,
    },
}
```

This is deferred until the first non-YAML host is implemented.

---

## Testing strategy

### Unit tests (per-profile)

Each profile file carries its own `#[cfg(test)]` module with dialect-specific
fixtures. For GitHub Actions:

- Resolve `key.jobs::key.build::key.steps[1]::key.run//bash::fn.setup`
  → correct span within the `run:` block.
- Compute tree_path for a Bash function inside a `run:` block → includes
  `//bash` marker.
- Roundtrip: resolve → compute → resolve produces the same span.
- Profile detection: `.github/workflows/ci.yml` matches,
  `kubernetes/deployment.yaml` does not.

### Span translation tests

- Verify that inner-parser line 1 maps to the correct outer-file line.
- Verify with block scalars that have `|`, `>`, and indentation.
- Verify with inline scalars.

### Ancestor matching tests

- `run:` under `jobs:` → fires.
- `run:` at top level → does not fire.
- `run:` under `jobs:` but with intervening keys → fires (subsequence, not
  exact path).

### Integration tests

- End-to-end: given a `.github/workflows/ci.yml` fixture, `liyi init` +
  `liyi check` exercises the full injection pipeline.
- `sibling_scan` on injected items: verify that hash-based reanchoring works
  across the injection boundary.

---

## AIGC Disclaimer

This document contains content from the following AI agents:

* Claude Opus 4.6

The document is authored by Claude Opus 4.6 with the human designer's input.
