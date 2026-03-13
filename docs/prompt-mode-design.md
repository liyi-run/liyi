<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# `--prompt` Mode Design

**Status:** Implemented  
**Target:** v0.1.x (M5.3)  
**Design authority:** `docs/liyi-design.md` v8.10

**Scope note (v8.10):** This document covers the initial `--prompt` scope: coverage gaps (Untracked, MissingRelatedEdge, ReqNoRelated). The cognitive load inversion principle (design doc v8.10, *The cognitive load inversion: tool-guided agents*) calls for extending `--prompt` to all diagnostics — stale items, shifted spans, unreviewed specs — each with per-item resolution instructions. The generalized `--prompt` design is deferred to a future revision of this document.

---

## Overview

The `--prompt` flag for `liyi check` emits structured JSON listing every coverage gap with resolution instructions. Unlike the default human-readable output or the planned `--json` mode (machine-readable for CI/dashboards), `--prompt` is specifically designed for agent consumption — it includes natural-language instructions for resolving each gap.

The formal schema is at `schema/prompt.schema.json`.

## Goals

1. **Agent-consumable:** The output is structured enough for an agent to parse programmatically, but also includes human-readable instructions for context.
2. **Self-contained:** Each gap entry contains all information needed to resolve it without additional file discovery.
3. **Actionable:** Instructions are specific and actionable — "Add X to Y" not "Something is wrong."

## Non-Goals

1. Not a different check — `--prompt` uses the same detection engine as default mode, just formats output differently.
2. Not for human terminal use — the default mode remains the human-friendly output.
3. Not a replacement for `--json` — `--json` (post-MVP) will be leaner, without instruction text, for CI/dashboard consumption.

## Output Schema

```jsonc
{
  "$schema": "https://liyi.run/schema/0.1/prompt.schema.json",
  "version": "0.1",
  "root": ".",
  "items": [
    {
      "type": "missing_requirement_spec",
      "requirement": "auth-check",
      "source_file": "src/auth/middleware.rs",
      "annotation_line": 15,
      "expected_sidecar": "src/auth/middleware.rs.liyi.jsonc",
      "instruction": "Add a requirementSpec with \"requirement\": \"auth-check\" and \"source_span\" covering the @liyi:requirement block at line 15."
    },
    {
      "type": "missing_related_edge",
      "requirement": "auth-check",
      "source_file": "src/auth/middleware.rs",
      "annotation_line": 42,
      "enclosing_item": "verify_session",
      "expected_sidecar": "src/auth/middleware.rs.liyi.jsonc",
      "instruction": "In the itemSpec for \"verify_session\", add \"related\": {\"auth-check\": null}."
    },
    {
      "type": "req_no_related",
      "requirement": "auth-check",
      "source_file": "src/auth/middleware.rs",
      "annotation_line": 15,
      "expected_sidecar": "src/auth/middleware.rs.liyi.jsonc",
      "instruction": "Requirement \"auth-check\" has no items referencing it via \"related\". Add a \"related\": {\"auth-check\": null} edge to at least one itemSpec that depends on this requirement."
    }
  ],
  "exit_code": 1
}
```

### Field Definitions

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `version` | string | Yes | Schema version, always `"0.1"` |
| `root` | string | No | Repository root relative to working directory (default: `"."`) |
| `items` | array | Yes | List of coverage gap diagnostics |
| `items[].type` | string | Yes | One of `"missing_requirement_spec"`, `"missing_related_edge"`, or `"req_no_related"` |
| `items[].requirement` | string | Yes | Name of the requirement |
| `items[].source_file` | string | Yes | Repo-relative path to source file containing the annotation |
| `items[].annotation_line` | integer | Yes | 1-indexed line number of the `@liyi:requirement` or `@liyi:related` marker |
| `items[].expected_sidecar` | string | Yes | Repo-relative path to the sidecar file that should contain the spec/edge |
| `items[].enclosing_item` | string | `missing_related_edge` only | The name of the item whose spec should contain the edge; present only when `type` is `"missing_related_edge"` |
| `items[].requirement_text` | string | No | The full text of the `@liyi:requirement` block, when available. Included so agents need not re-read the source file. |
| `items[].instruction` | string | Yes | Natural-language instruction for resolving the gap |
| `exit_code` | integer | Yes | Exit code that `liyi check` would return (0, 1, or 2) |

**Note on non-coverage diagnostics:** Error-class diagnostics (`ParseError`, `OrphanedSource`, `SpanPastEof`, etc.) are not included in `items`. They affect the process exit code (which is reflected in `exit_code`) but are not actionable coverage gaps. Agents encountering `exit_code: 2` should fall back to default output mode for error details.

## Implementation Plan

### 1. Add `--prompt` CLI flag

In `crates/liyi-cli/src/cli.rs`, add to the `Check` variant:

```rust
/// Emit agent-consumable JSON output for coverage gaps
#[arg(long)]
prompt: bool,
```

Note: `--prompt` and `--json` (future) will be mutually exclusive. Add `conflicts_with = "json"` when `--json` is implemented.

### 2. Update `CheckFlags`

In `crates/liyi/src/diagnostics.rs`:

```rust
#[derive(Debug, Clone)]
pub struct CheckFlags {
    // ... existing fields ...
    pub prompt_mode: bool, // New: indicates --prompt was requested
}
```

Or alternatively, handle `--prompt` entirely at the CLI layer by passing a formatter enum to `run_check`.

### 3. Create prompt output types

In `crates/liyi/src/` (new file `prompt.rs` or in `diagnostics.rs`):

```rust
#[derive(Serialize)]
pub struct PromptOutput {
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    pub items: Vec<PromptItem>,
    pub exit_code: u8,
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum PromptItem {
    #[serde(rename = "missing_requirement_spec")]
    MissingRequirementSpec {
        requirement: String,
        source_file: String,
        annotation_line: usize,
        expected_sidecar: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        requirement_text: Option<String>,
        instruction: String,
    },
    #[serde(rename = "missing_related_edge")]
    MissingRelatedEdge {
        requirement: String,
        source_file: String,
        annotation_line: usize,
        enclosing_item: String,
        expected_sidecar: String,
        instruction: String,
    },
    #[serde(rename = "req_no_related")]
    ReqNoRelated {
        requirement: String,
        source_file: String,
        annotation_line: usize,
        expected_sidecar: String,
        instruction: String,
    },
}
```

### 4. Generate instructions

Instruction templates:

**Missing requirement spec:**
```
Add a requirementSpec with "requirement": "{name}" and "source_span" covering the @liyi:requirement block at line {line}.
```

**Missing related edge:**
```
In the itemSpec for "{item}", add "related": {"{name}": null}.
```

**Requirement with no related items:**
```
Requirement "{name}" has no items referencing it via "related". Add a "related": {"{name}": null} edge to at least one itemSpec that depends on this requirement.
```

### 5. Modify `run_check` to support prompt mode

Option A: Return diagnostics + optional prompt output
```rust
pub fn run_check(
    root: &Path,
    scope_paths: &[PathBuf],
    fix: bool,
    dry_run: bool,
    flags: &CheckFlags,
) -> (Vec<Diagnostic>, LiyiExitCode, Option<PromptOutput>)
```

Option B: Handle formatting at CLI layer
```rust
// In main.rs
let (diagnostics, exit_code) = liyi::check::run_check(...);

if prompt {
    let prompt_output = build_prompt_output(&diagnostics, exit_code, root);
    println!("{}", serde_json::to_string_pretty(&prompt_output).unwrap());
} else {
    // Default human-readable output
    for d in &diagnostics { ... }
}
```

Option B is cleaner — keeps formatting concerns out of the core check logic.

### 6. Build prompt output from diagnostics

The CLI layer iterates over diagnostics and builds `PromptItem` entries for:
- `DiagnosticKind::Untracked` → `MissingRequirementSpec`
- `DiagnosticKind::MissingRelatedEdge` → `MissingRelatedEdge`
- `DiagnosticKind::ReqNoRelated` → `ReqNoRelated`

Other diagnostics (Stale, Shifted, etc.) are not coverage gaps and don't appear in `--prompt` output in this scope. Error-class diagnostics (`ParseError`, `OrphanedSource`, etc.) are not emitted as items but still affect `exit_code`.

### 7. Exit code handling

`--prompt` output includes the exit code that would be returned. The actual process exit code is the same as default mode (respecting `--fail-on-*` flags).

## Acceptance Criteria

1. `liyi check --prompt` on a fixture with gaps produces valid JSON matching `schema/prompt.schema.json`.
2. `liyi check --prompt` on a clean repo produces `{"version": "0.1", "items": [], "exit_code": 0}`.
3. The JSON includes all three gap types: `missing_requirement_spec`, `missing_related_edge`, and `req_no_related`.
4. `--prompt` will be mutually exclusive with `--json` (when implemented).
5. Exit code behavior is identical to default mode (respects all `--fail-on-*` flags).
6. When error-class diagnostics are present, `exit_code` is `2` even if the `items` array is empty.

## Testing Strategy

1. **Golden-file fixtures:**
   - `prompt_output/mixed_gaps/` — fixture with all three gap types present.
   - `prompt_output/clean/` — fixture with no gaps (empty `items` array).
   - `prompt_output/errors_only/` — fixture with `ParseError` or `OrphanedSource` but no coverage gaps (verifies `exit_code: 2` with empty `items`).
   - `prompt_output/multi_file/` — gaps spread across multiple files.
2. **Unit tests:** Verify instruction generation for each of the three gap types.
3. **Integration test:** Parse `--prompt` output and validate against `schema/prompt.schema.json`.
4. **Instruction accuracy test:** For each instruction template, apply the described mutation and verify that a follow-up `liyi check` no longer reports the gap.

## Resolved Questions

1. **Should `--prompt` include non-coverage diagnostics?** No — only Untracked, MissingRelatedEdge, and ReqNoRelated. Stale/Shifted/Unreviewed will be added when `--prompt` is generalized to all diagnostics (deferred). Error-class diagnostics are not emitted as items but affect `exit_code`.

2. **Should we include the actual requirement text in `missing_requirement_spec`?** Yes. The cognitive load inversion principle argues for including all context the tool already has, so agents need not re-read files. Added as optional `requirement_text` field.

3. **What about multiple gaps for the same requirement?** Each gap is listed separately. Agents can deduplicate if needed.

## Future Extensions

When `--prompt` is generalized to all diagnostics, the `items` array will include additional types. Sketched here for schema compatibility planning (exact fields TBD):

| Future `type` value | Source diagnostic | Example instruction |
|---|---|---|
| `stale_spec` | `Stale` | Re-read `{source_file}` lines {start}–{end} and update the intent for "{item}" in `{sidecar}`. |
| `shifted_span` | `Shifted` | Run `liyi check --fix` to auto-correct the span for "{item}" from [{old}] to [{new}]. |
| `unreviewed_spec` | `Unreviewed` | Run `liyi approve {sidecar} {item}` after verifying the intent is correct. |

Other planned extensions:

- Include `suggested_intent` field (agent-generated) for missing requirement specs.
- Support for `--prompt` consuming a triage report instead of running check.

---

## AIGC Disclaimer

This document contains content from the following AI agents:

* Claude Opus 4.6
* Kimi K2.5

The initial draft was primarily authored by Kimi K2.5 with the human designer's input. Design review and revisions (schema rename, ReqNoRelated coverage, resolved open questions, future extension stubs, formal JSON Schema) by Claude Opus 4.6.
