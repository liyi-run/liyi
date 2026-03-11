<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# `--prompt` Mode Design

**Status:** Design complete, awaiting implementation  
**Target:** v0.1.x (M5.3)  
**Design authority:** `docs/liyi-design.md` v8.10

**Scope note (v8.10):** This document covers the initial `--prompt` scope: coverage gaps (Untracked, MissingRelatedEdge). The cognitive load inversion principle (design doc v8.10, *The cognitive load inversion: tool-guided agents*) calls for extending `--prompt` to all diagnostics — stale items, shifted spans, unreviewed specs — each with per-item resolution instructions. The generalized `--prompt` design is deferred to a future revision of this document.

---

## Overview

The `--prompt` flag for `liyi check` emits structured JSON listing every coverage gap with resolution instructions. Unlike the default human-readable output or the planned `--json` mode (machine-readable for CI/dashboards), `--prompt` is specifically designed for agent consumption — it includes natural-language instructions for resolving each gap.

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
  "version": "0.1",
  "gaps": [
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
    }
  ],
  "exit_code": 1
}
```

### Field Definitions

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `version` | string | Yes | Schema version, always "0.1" |
| `gaps` | array | Yes | List of coverage gaps |
| `gaps[].type` | string | Yes | `"missing_requirement_spec"` or `"missing_related_edge"` |
| `gaps[].requirement` | string | Yes | Name of the requirement |
| `gaps[].source_file` | string | Yes | Repo-relative path to source file containing the annotation |
| `gaps[].annotation_line` | number | Yes | 1-indexed line number of the `@liyi:requirement` or `@liyi:related` marker |
| `gaps[].expected_sidecar` | string | Yes | Repo-relative path to the sidecar file that should contain the spec/edge |
| `gaps[].enclosing_item` | string | Yes for `missing_related_edge` | The name of the item whose spec should contain the edge; required when `type` is `missing_related_edge` |
| `gaps[].instruction` | string | Yes | Natural-language instruction for resolving the gap |
| `exit_code` | number | Yes | Exit code that `liyi check` would return (0, 1, or 2) |

## Implementation Plan

### 1. Add `--prompt` CLI flag

In `crates/liyi-cli/src/cli.rs`, add to the `Check` variant:

```rust
/// Emit agent-consumable JSON output for coverage gaps
#[arg(long, conflicts_with = "json")]
prompt: bool,
```

Note: `--prompt` and `--json` (future) are mutually exclusive.

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
    pub gaps: Vec<PromptGap>,
    pub exit_code: u8,
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum PromptGap {
    #[serde(rename = "missing_requirement_spec")]
    MissingRequirementSpec {
        requirement: String,
        source_file: String,
        annotation_line: usize,
        expected_sidecar: String,
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
    let prompt_output = build_prompt_output(&diagnostics, exit_code);
    println!("{}", serde_json::to_string_pretty(&prompt_output).unwrap());
} else {
    // Default human-readable output
    for d in &diagnostics { ... }
}
```

Option B is cleaner — keeps formatting concerns out of the core check logic.

### 6. Build prompt output from diagnostics

The CLI layer iterates over diagnostics and builds `PromptGap` entries for:
- `DiagnosticKind::Untracked` → `MissingRequirementSpec`
- `DiagnosticKind::MissingRelatedEdge` → `MissingRelatedEdge`

Other diagnostics (Stale, Shifted, etc.) are not coverage gaps and don't appear in `--prompt` output.

### 7. Exit code handling

`--prompt` output includes the exit code that would be returned. The actual process exit code is the same as default mode (respecting `--fail-on-*` flags).

## Acceptance Criteria

1. `liyi check --prompt` on a fixture with gaps produces valid JSON matching the schema.
2. `liyi check --prompt` on a clean repo produces `{"version": "0.1", "gaps": [], "exit_code": 0}`.
3. The JSON includes both `missing_requirement_spec` and `missing_related_edge` gap types.
4. `--prompt` is mutually exclusive with `--json` (when implemented).
5. Exit code behavior is identical to default mode (respects all `--fail-on-*` flags).

## Testing Strategy

1. **Golden-file fixture:** `prompt_output/` with mixed gaps.
2. **Unit tests:** Verify instruction generation for each gap type.
3. **Integration test:** Parse `--prompt` output and verify schema compliance.

## Open Questions

1. **Should `--prompt` include non-coverage diagnostics?** Currently no — only Untracked and MissingRelatedEdge. Stale/Shifted/Unreviewed are not "coverage gaps" in the same sense.

2. **Should we include the actual requirement text in `missing_requirement_spec`?** Could help agents understand context without re-reading the file. Trade-off: larger output vs. more context.

3. **What about multiple gaps for the same requirement?** The current design lists each gap separately. This is fine — agents can deduplicate if needed.

## Future Extensions

- Include `suggested_intent` field (agent-generated) for missing requirement specs
- Include `related_items` count for requirements with no edges (ReqNoRelated)
- Support for `--prompt` consuming a triage report instead of running check

---

*Design document for `--prompt` mode implementation.*
