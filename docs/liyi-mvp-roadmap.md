# 立意 (Lìyì) — MVP Implementation Roadmap

2026-03-05 (updated 2026-03-06)

---

## Overview

This document is the implementation plan for 立意 v0.1 — the CI linter, the spec convention, the agent instruction, and enough supporting artifacts to dogfood on the project's own codebase. The scope is levels 0–3 of the adoption ladder, plus the convention foundation for levels 4–5.

**Deliverables:**

1. `liyi check` — the CI linter binary (Rust) ✅
2. `liyi reanchor` — the span re-hashing tool (subcommand of the same binary) ✅
3. `liyi.schema.json` — the JSON Schema for `.liyi.jsonc` v0.1 ✅
4. Agent instruction — the ~12-line AGENTS.md paragraph ✅
5. Demo repo — the linter's own codebase, dogfooded with `.liyi.jsonc` specs and `@liyi:module` markers ✅
6. README / landing page — the one-paragraph pitch + progressive adoption table ✅

---

## Current Status (2026-03-06)

### Components — all implemented

| Module | Status | Notes |
|--------|--------|-------|
| `cli.rs` | ✅ Done | `check` + `reanchor` subcommands, all planned flags |
| `discovery.rs` | ✅ Done | `.liyiignore` support, ambiguous sidecar detection, scope filtering |
| `sidecar.rs` | ✅ Done | JSONC comment stripping, serde, `deny_unknown_fields` |
| `markers.rs` | ✅ Done | All 7 marker types, fullwidth normalization, multilingual aliases |
| `hashing.rs` | ✅ Done | SHA-256, CRLF normalization, all `SpanError` variants |
| `shift.rs` | ✅ Done | ±100-line scan with anchor hint shortcut |
| `check.rs` | ✅ Done | Two-pass logic, `--fix` write-back |
| `reanchor.rs` | ✅ Done | Targeted + batch re-hashing, `--migrate` scaffold |
| `diagnostics.rs` | ✅ Done | All diagnostic types, formatting, exit codes |
| `schema.rs` | ✅ Done | Accepts `"0.1"` only, migration scaffold |
| JSON Schema | ✅ Done | `schema/liyi.schema.json` |
| AGENTS.md | ✅ Done | Dogfooded on the project itself |
| README (en + zh) | ✅ Done | WIP badge, adoption table, CLI reference |
| Cargo workspace | ✅ Done | `crates/liyi` (library) + `crates/liyi-cli` (binary) |

### Diagnostics — wiring gaps

Four `DiagnosticKind` variants are defined but never emitted:

| Variant | Status | What's needed |
|---------|--------|---------------|
| `RequirementCycle` | ❌ Never emitted | Add cycle detection in pass 2 when resolving `related` edges transitively |
| `Untracked` | ❌ Never emitted | After pass 1, flag requirements found in source markers but absent from any sidecar |
| `ReqNoRelated` | ❌ Never emitted | After pass 2, flag requirements that no item references (informational) |
| `MalformedHash` | ❌ Never emitted | Validate `source_hash` format (`^sha256:[0-9a-f]+$`) during sidecar parsing or check |

### Golden-file test coverage

| Fixture | Status | Notes |
|---------|--------|-------|
| `basic_pass/` | ✅ | |
| `stale_hash/` | ✅ | |
| `shifted_span/` | ✅ | Includes `shifted_span_fix` variant |
| `unreviewed/` | ✅ | Tests lenient + strict modes |
| `orphaned_source/` | ✅ | |
| `malformed_jsonc/` | ✅ | |
| `trivial_ignore/` | ✅ | |
| `span_past_eof/` | ✅ | |
| `fullwidth_markers/` | ✅ | |
| `multilingual_aliases/` | ✅ | |
| `req_changed/` | ❌ Missing | Need fixture to test `ReqChanged` diagnostic |
| `req_cycle/` | ❌ Missing | Need fixture + cycle detection logic first |
| `liyiignore/` | 🟡 | Covered by `discover_respects_liyiignore` unit test, no golden fixture |

### Other gaps

| Item | Status | Notes |
|------|--------|-------|
| `shift_proptest.rs` | ❌ Missing | Property-based tests for shift detection |
| CI (GitHub Actions) | ❌ Not set up | Workflow to run `cargo test` + `liyi check` |
| Dogfooding locally | ✅ Done | Full loop confirmed: agent changes code → `liyi check` detects staleness → agent reanchors specs. No human instruction needed beyond the initial prompt. CI not yet wired. |
| Summary line output | ❌ Not implemented | "3 stale, 1 unreviewed, 12 current" after diagnostics |

---

## Architecture

### System diagram

```
┌──────────────────────────────────────────────────────────────────┐
│                         Repository                               │
│                                                                  │
│  src/billing/                                                    │
│  ├── README.md              ← @liyi:module marker                │
│  ├── money.rs               ← source (any language)              │
│  ├── money.rs.liyi.jsonc    ← item-level specs (sidecar)         │
│  ├── orders.rs                                                   │
│  ├── orders.rs.liyi.jsonc                                        │
│  └── .liyiignore            ← file-level exclusions              │
│                                                                  │
│  AGENTS.md                  ← agent instruction (~12 lines)      │
└──────────┬────────────┬──────────────────────────────────────────┘
           │            │
     ┌─────▼─────┐ ┌───▼────────────────┐
     │ liyi check│ │ Agent (any LLM)    │
     │           │ │                    │
     │ Pass 1:   │ │ Reads AGENTS.md →  │
     │  Walk repo│ │ infers intent →    │
     │  Find all │ │ writes .liyi.jsonc │
     │  @liyi:   │ │ (source_span only) │
     │  require- │ │                    │
     │  ment     │ │ Annotates trivial  │
     │  markers  │ │ items with         │
     │           │ │ @liyi:trivial      │
     │ Pass 2:   │ │                    │
     │  For each │ └────────────────────┘
     │  .liyi.   │
     │  jsonc:   │         ┌────────────────────┐
     │  - hash   │         │ liyi reanchor      │
     │    spans  │         │                    │
     │  - check  │         │ Fills source_hash, │
     │    review │         │ source_anchor from │
     │  - resolve│         │ actual source file │
     │    related│         │ bytes. No LLM.     │
     │    edges  │         └────────────────────┘
     │           │
     │ Exit 0/1/2│
     └───────────┘
```

### Binary: `liyi`

A single Rust binary with subcommands:

| Subcommand | Purpose |
|---|---|
| `liyi check [paths...]` | Lint: staleness, review status, requirement tracking |
| `liyi check --fix` | Lint + auto-correct shifted spans, fill missing hashes |
| `liyi approve [paths...] [--yes]` | Interactive review: mark specs as human-approved |
| `liyi init [source-file]` | Scaffold AGENTS.md or skeleton `.liyi.jsonc` sidecar |
| `liyi reanchor <sidecar> [--item <name> --span <s,e>]` | Manual span re-hashing for targeted fixes |
| `liyi reanchor --migrate` | Schema version migration (no-op in 0.1, scaffolded) |

### Crate structure

```
liyi/
├── Cargo.toml               ← workspace root
├── crates/
│   ├── liyi/                ← library crate
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── check.rs         ← Core check logic (two-pass)
│   │   │   ├── discovery.rs     ← File walking, .gitignore/.liyiignore filtering
│   │   │   ├── sidecar.rs       ← .liyi.jsonc parsing & serialization (serde)
│   │   │   ├── markers.rs       ← Source marker scanning (@liyi:*, normalization)
│   │   │   ├── hashing.rs       ← source_span → SHA-256, anchor extraction
│   │   │   ├── shift.rs         ← Span-shift detection
│   │   │   ├── reanchor.rs      ← reanchor subcommand logic
│   │   │   ├── diagnostics.rs   ← Diagnostic types, formatting, exit codes
│   │   │   └── schema.rs        ← Version validation, migration scaffold
│   │   └── tests/
│   │       ├── golden.rs        ← Golden-file test runner
│   │       └── fixtures/        ← Golden-file test repos
│   └── liyi-cli/            ← binary crate
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs          ← CLI entry point
│           └── cli.rs           ← Argument parsing (clap derive)
│
├── schema/
│   └── liyi.schema.json     ← JSON Schema for .liyi.jsonc v0.1
│
├── AGENTS.md                ← Agent instruction (dogfooded)
├── README.md                ← Project README with @liyi:module
└── crates/liyi/src/*.rs.liyi.jsonc  ← Dogfood: linter's own specs
```
│   ├── fixtures/            ← Golden-file test repos (see Current Status for coverage)
│   ├── golden.rs            ← Golden-file test runner
│   └── shift_proptest.rs    ← Property-based tests for span-shift (planned)
```

---

## Component Breakdown

### 1. `cli.rs` — CLI & Argument Parsing

**Purpose:** Parse command-line arguments & dispatch to subcommands.

**Dependencies:** `clap` (derive API).

**Interface:**

```rust
#[derive(Parser)]
#[command(name = "liyi", about = "立意 — establish intent before execution")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check specs for staleness, review status, and requirement tracking
    Check {
        /// Paths to check (default: CWD, recursive)
        paths: Vec<PathBuf>,

        /// Auto-correct shifted spans and fill missing hashes
        #[arg(long)]
        fix: bool,

        /// Fail if any reviewed spec is stale (default: true)
        #[arg(long, default_value_t = true)]
        fail_on_stale: bool,

        /// Fail if specs exist without review (default: false)
        #[arg(long, default_value_t = false)]
        fail_on_unreviewed: bool,

        /// Fail if a reviewed spec references a changed requirement (default: true)
        #[arg(long, default_value_t = true)]
        fail_on_req_changed: bool,

        /// Override repo root (default: walk up to .git/)
        #[arg(long)]
        root: Option<PathBuf>,
    },

    /// Re-hash source spans in sidecar files
    Reanchor {
        /// Sidecar file to reanchor
        file: PathBuf,

        /// Target a specific item by name
        #[arg(long)]
        item: Option<String>,

        /// Override span (start,end)
        #[arg(long, value_parser = parse_span)]
        span: Option<(usize, usize)>,

        /// Migrate sidecar to current schema version
        #[arg(long)]
        migrate: bool,
    },
}
```

**Constraints:**
- Positional `paths` in `check` scope item checking only (pass 2). Pass 1 (requirement discovery) always walks the full repo root.
- `--root` overrides `.git/`-based discovery for non-git repos.
- Boolean flags use `--no-fail-on-stale` to disable (clap default negation).

**Size estimate:** ~80 lines.

---

### 2. `discovery.rs` — File Walking & Filtering

**Purpose:** Walk the repo tree, discover `.liyi.jsonc` sidecars and source files, respecting `.gitignore` and `.liyiignore`.

**Dependencies:** `ignore` crate (the same crate `ripgrep` uses — handles `.gitignore` natively, supports custom ignore files).

**Key types:**

```rust
/// A discovered sidecar and its associated source file.
struct SidecarEntry {
    sidecar_path: PathBuf,      // e.g., src/billing/money.rs.liyi.jsonc
    source_path: PathBuf,       // e.g., src/billing/money.rs (derived)
    repo_relative_source: String, // e.g., "src/billing/money.rs"
}

/// Walk results.
struct DiscoveryResult {
    sidecars: Vec<SidecarEntry>,
    /// Files containing @liyi:requirement markers (discovered in pass 1)
    requirement_files: Vec<PathBuf>,
}
```

**Behavior:**

1. Locate repo root: walk up from CWD (or `--root`) looking for `.git/`.
2. Build an `ignore::WalkBuilder` rooted at the repo root.
3. Add `.liyiignore` as a custom ignore filename (the `ignore` crate supports this natively via `add_custom_ignore_filename`).
4. Walk the tree:
   - Collect all `*.liyi.jsonc` files → derive source path by stripping `.liyi.jsonc` suffix.
   - Collect all non-ignored text files for marker scanning (pass 1).
5. For pass 2 scoping: filter sidecars to those whose source_path falls under the CLI-specified `paths`.

**Constraints:**
- The `ignore` crate handles `.gitignore` cascading, negation patterns, and parent directory inheritance — we do **not** reimplement this.
- `.liyiignore` follows identical semantics to `.gitignore` (the `ignore` crate supports this natively).
- Sidecar naming: `<source_filename>.liyi.jsonc`. The discovery module validates this — if `money.liyi.jsonc` exists alongside `money.rs.liyi.jsonc`, emit the ambiguous-sidecar warning.

**Size estimate:** ~120 lines.

---

### 3. `sidecar.rs` — `.liyi.jsonc` Parsing & Serialization

**Purpose:** Parse and write `.liyi.jsonc` sidecar files. Typed representation of the schema.

**Dependencies:** `serde`, `serde_json` (with JSONC support — strip comments before parsing, or use a JSONC-aware parser like `json_comments`).

**Key types:**

```rust
#[derive(Deserialize, Serialize)]
struct Sidecar {
    version: String,
    source: String,
    specs: Vec<Spec>,
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
enum Spec {
    Item(ItemSpec),
    Requirement(RequirementSpec),
}

#[derive(Deserialize, Serialize)]
struct ItemSpec {
    item: String,
    intent: String,
    source_span: [usize; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    source_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_anchor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reviewed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    related: Option<HashMap<String, Option<String>>>,
}

#[derive(Deserialize, Serialize)]
struct RequirementSpec {
    requirement: String,
    source_span: [usize; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    source_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_anchor: Option<String>,
}
```

**Behavior:**
- Parse: strip JSONC comments (lines starting with `//` after trimming, and `/* */` blocks), then `serde_json::from_str`. Report parse errors with file path and position.
- Validate: reject unknown `"version"` values (only `"0.1"` accepted). Report malformed `source_hash` (must match `^sha256:[0-9a-f]+$`). Report inverted/zero-length spans.
- Serialize: write back with `serde_json::to_string_pretty`, preserving field order. Prepend the informational JSONC header comment.
- Distinguish `ItemSpec` from `RequirementSpec` by the presence of `"item"` vs `"requirement"` key (serde untagged enum with field-based disambiguation).

**Constraints:**
- `source_span` values are 1-indexed. The parser does not convert to 0-indexed — all internal logic uses 1-indexed to match the schema, editor line numbers, and `git blame`.
- Unknown fields: `serde(deny_unknown_fields)` on each struct — fail loudly on unexpected keys rather than silently ignoring them. This prevents typos (`"intnet"`) from being silently accepted.

**Size estimate:** ~100 lines.

---

### 4. `markers.rs` — Source Marker Scanning & Normalization

**Purpose:** Scan source file lines for `@liyi:*` markers. Handle full-width/half-width normalization and multilingual aliases.

**Key types:**

```rust
/// A discovered marker in a source file.
enum SourceMarker {
    Module { line: usize },
    Requirement { name: String, line: usize },
    Related { name: String, line: usize },
    Intent { prose: Option<String>, is_doc: bool, line: usize },
    Trivial { line: usize },
    Ignore { reason: Option<String>, line: usize },
    Nontrivial { line: usize },
}
```

**Behavior:**

1. **Normalize.** On each scanned line, replace the four full-width characters with half-width equivalents before matching:
   - `＠` (U+FF20) → `@` (U+0040)
   - `：` (U+FF1A) → `:` (U+003A)
   - `（` (U+FF08) → `(` (U+0028)
   - `）` (U+FF09) → `)` (U+0029)

2. **Alias table.** A static `const` array mapping all known marker strings to their canonical form. The complete alias set (from the design doc's multilingual table):

   | Canonical (English) | Aliases |
   |---|---|
   | `@liyi:ignore` | `@立意:忽略`, `@liyi:ignorar`, `@立意:無視`, `@liyi:ignorer`, `@립의:무시` |
   | `@liyi:trivial` | `@立意:显然`, `@立意:自明`, `@립의:자명` |
   | `@liyi:nontrivial` | `@立意:并非显然`, `@liyi:notrivial`, `@立意:非自明`, `@liyi:nãotrivial`, `@립의:비자명` |
   | `@liyi:module` | `@立意:模块`, `@liyi:módulo`, `@立意:モジュール`, `@립의:모듈` |
   | `@liyi:requirement` | `@立意:需求`, `@liyi:requisito`, `@立意:要件`, `@liyi:exigence`, `@립의:요건` |
   | `@liyi:related` | `@立意:有关`, `@liyi:relacionado`, `@立意:関連`, `@liyi:lié`, `@립의:관련` |
   | `@liyi:intent` | `@立意:意图`, `@liyi:intención`, `@立意:意図`, `@liyi:intention`, `@립의:의도`, `@liyi:intenção` |

   Implementation: build a `HashMap<&str, MarkerKind>` at startup (or `phf` for compile-time). Scan each normalized line for any of the key strings via `str::contains` or a single regex alternation.

3. **Name extraction for `@liyi:requirement` and `@liyi:related`:**
   - If next non-whitespace after keyword is `(` or `（` (already normalized to `(`): name is everything inside matching `)`.
   - Otherwise: name is first whitespace-delimited token.

4. **`@liyi:intent` extraction:**
   - If followed by `=doc` (or `=文档`): `is_doc = true`, no prose.
   - Otherwise: remaining text on same line + contiguous comment lines below = prose.

**Constraints:**
- Normalization applies only to lines being scanned, not entire files.
- The alias table is `const` — no runtime configuration, no locale files.
- Both prefix forms accepted: `@立意:忽略` and `@liyi:忽略`. The scanner matches the full `@prefix:keyword` string against the alias set.

**Size estimate:** ~150 lines.

---

### 5. `hashing.rs` — Source Hashing & Anchor Extraction

**Purpose:** Read source lines at a given span, normalize line endings, compute SHA-256, extract anchor.

**Dependencies:** `sha2` crate.

**Key function:**

```rust
/// Hash the source lines at [start, end] (1-indexed, inclusive).
/// Returns (hash, anchor) or an error if span is invalid.
fn hash_span(
    source_content: &str,
    span: [usize; 2],
) -> Result<(String, String), SpanError> {
    // ...
}

enum SpanError {
    PastEof { span: [usize; 2], file_lines: usize },
    Inverted { span: [usize; 2] },
    Empty { span: [usize; 2] },
}
```

**Behavior:**

1. Split source content into lines.
2. Validate span: `start <= end`, `start >= 1`, `end <= line_count`. Report structured errors otherwise.
3. Extract lines `[start-1..end]` (0-indexed slice of the 1-indexed interval).
4. Join with `\n` (LF normalization — strip any `\r`).
5. SHA-256 hash the joined bytes → `format!("sha256:{:x}", digest)`.
6. Anchor = first line of the span (trimmed? no — literal text as-is).

**Constraints:**
- Line ending normalization to LF is mandatory for cross-platform consistency.
- No trimming of the anchor — it's the literal text for grep-based shift detection.
- The hash is always lowercase hex.

**Size estimate:** ~50 lines.

---

### 6. `shift.rs` — Span-Shift Detection

**Purpose:** When a hash mismatch is detected, scan nearby lines for the original content to determine if the span shifted rather than changed.

**Algorithm:**

```
Given: spec with source_span [s, e] and recorded source_hash H.
       Actual hash at [s, e] is H' ≠ H.
       span_length = e - s + 1

1. Search window: max(1, s - 100) to min(file_lines, e + 100).
2. For each offset d in [−100, +100]:
     candidate_start = s + d
     candidate_end   = e + d
     if candidate is valid (≥ 1, ≤ file_lines):
       compute hash of [candidate_start, candidate_end]
       if hash == H:
         return ShiftResult::Shifted { delta: d, new_span: [candidate_start, candidate_end] }
3. Return ShiftResult::Stale
```

**Delta propagation optimization:**

Once a shift delta is established for one item in a file, subsequent items in the same file are checked at `span + delta` first, before scanning the full window. This handles the common case (single insertion/deletion shifting all spans uniformly) in O(1) per item instead of O(200).

```rust
enum ShiftResult {
    Shifted { delta: i64, new_span: [usize; 2] },
    Stale,
}
```

**Constraints:**
- Window size: ±100 lines (hardcoded in 0.1; configurable post-MVP if needed).
- If the file is shorter than the window, clamp to file boundaries.
- Shift detection is best-effort — it's a heuristic (same as `patch(1)` fuzz). False negatives (reports STALE when it's actually shifted) are safe; false positives (reports SHIFTED when content actually changed) are extremely unlikely given SHA-256.

**Size estimate:** ~80 lines.

---

### 7. `check.rs` — Core Check Logic (Two-Pass)

**Purpose:** The heart of `liyi check`. Orchestrates discovery, scanning, hashing, and diagnostics.

**Two-pass design:**

**Pass 1 — Requirement discovery (project-global):**

1. Walk the entire repo root (ignoring `.gitignore`/`.liyiignore` matches).
2. For every non-binary file, scan for `@liyi:requirement` markers.
3. For each discovered requirement:
   - Record its name, file path, and line number.
   - Hash the requirement block `source_span` from the corresponding `.liyi.jsonc` (if it exists) to get the current requirement hash.
   - If no sidecar entry exists for the requirement, record it as `UNTRACKED`.
4. Build a `HashMap<String, RequirementRecord>` — name → (hash, location).
5. Detect duplicate requirement names: if two markers declare the same name, emit an error.

**Pass 2 — Item checking (scoped to CLI paths):**

For each `.liyi.jsonc` in scope:

1. Parse the sidecar (→ `sidecar.rs`). On parse error, emit `EXIT_INTERNAL` diagnostic.
2. Validate version field. Unknown version → `EXIT_INTERNAL`.
3. Check source file exists. Missing → emit orphaned-source error.
4. For each `Spec::Item`:
   a. Read source file content (cache per file — don't re-read for every spec).
   b. Hash the `source_span` (→ `hashing.rs`). Handle `SpanError` variants.
   c. If `source_hash` is present and matches → CURRENT. If mismatch → attempt shift detection (→ `shift.rs`).
   d. Check review status: is `reviewed: true` in sidecar, OR does `@liyi:intent` exist within the `source_span`? (→ `markers.rs` scan of the span lines). Mark reviewed/unreviewed.
   e. Check `@liyi:trivial` / `@liyi:ignore` within or immediately before the span. If found, mark as trivial/ignored (skip review requirement).
   f. If `related` is present: for each requirement name, look up in the pass-1 map. If not found → `ERROR: unknown requirement`. If found and hash differs from recorded hash → `REQ CHANGED`.
5. For each `Spec::Requirement`:
   a. Hash the `source_span`. If `source_hash` present and mismatches → STALE (requirement text changed but sidecar not updated — run `liyi reanchor`).
6. Report requirements from pass 1 that have no referencing items (informational).

**`--fix` behavior (integrated into pass 2):**

When `--fix` is active:
- Fill in missing `source_hash` and `source_anchor` (same as `reanchor`).
- Auto-correct SHIFTED spans (write new span, recompute hash/anchor).
- Write modified sidecars back to disk.
- Do NOT modify `intent`, `reviewed`, `related`, or any human-authored field.

**Key types:**

```rust
struct CheckResult {
    diagnostics: Vec<Diagnostic>,
    exit_code: ExitCode, // 0, 1, or 2
}

enum ExitCode {
    Clean = 0,
    CheckFailure = 1,
    InternalError = 2,
}
```

**Constraints:**
- Pass 1 is always project-global regardless of CLI path args.
- File content is cached: read each source file at most once.
- Cycle detection in requirement hierarchies: track visited nodes during transitive `related` resolution. If a cycle is detected, emit error and stop traversing that path.
- Exit code 2 takes precedence over 1.

**Size estimate:** ~250 lines (the largest module).

---

### 8. `reanchor.rs` — Reanchor Subcommand

**Purpose:** Re-hash source spans in a sidecar file. Manual tool for fixing spans after line shifts.

**Behavior:**

1. Parse the target sidecar.
2. If `--item` and `--span` are specified: find the named item, update its span, recompute hash/anchor.
3. If neither: for every spec in the sidecar, recompute hash/anchor from the source file at the recorded span. This handles "code changed at the same span" (human confirms intent still holds → re-hash).
4. If `--migrate`: update `"version"` to current (no-op in 0.1, but the scaffold ensures the flag exists and the code path handles future versions).
5. Write modified sidecar back.

**Constraints:**
- `reanchor` never modifies `intent`, `reviewed`, or `related`.
- If the source file doesn't exist, emit an error (can't reanchor an orphaned spec).
- Idempotent: running twice produces the same output.

**Size estimate:** ~60 lines.

---

### 9. `diagnostics.rs` — Diagnostic Types & Formatting

**Purpose:** Structured diagnostic types, human-readable formatting, exit code computation.

**Key types:**

```rust
enum Severity {
    Info,
    Warning,
    Error,
}

struct Diagnostic {
    file: PathBuf,
    item_or_req: String,
    kind: DiagnosticKind,
    severity: Severity,
    message: String,
}

enum DiagnosticKind {
    Current,           // ✓ reviewed, current
    Unreviewed,        // ⚠ unreviewed
    Stale,             // ⚠ STALE
    Shifted { from: [usize; 2], to: [usize; 2] }, // ↕ SHIFTED
    ReqChanged { requirement: String },  // ⚠ REQ CHANGED
    UnknownRequirement { name: String }, // ✗ ERROR
    Untracked,         // ⚠ UNTRACKED
    ReqNoRelated,      // · requirement has no related items
    Trivial,           // · trivial
    Ignored,           // · ignored
    SpanPastEof { span: [usize; 2], file_lines: usize },
    InvalidSpan { span: [usize; 2] },
    MalformedHash,
    DuplicateEntry,
    OrphanedSource,
    ParseError { detail: String },
    UnknownVersion { version: String },
    RequirementCycle { path: Vec<String> },
    AmbiguousSidecar { canonical: String, other: String },
}
```

**Formatting:** Each diagnostic renders to one line with an icon prefix matching the design doc's output format:

```
add_money: ✓ reviewed, current
convert_currency: ⚠ STALE — source changed since spec was written
```

**Exit code computation:**

```rust
fn compute_exit_code(diagnostics: &[Diagnostic], flags: &CheckFlags) -> ExitCode {
    if diagnostics.iter().any(|d| d.severity == Severity::Error && matches!(d.kind, ParseError | UnknownVersion)) {
        ExitCode::InternalError // 2
    } else if /* any triggering condition based on flags */ {
        ExitCode::CheckFailure // 1
    } else {
        ExitCode::Clean // 0
    }
}
```

**Constraints:**
- Exit code 2 takes precedence over 1.
- `--fail-on-stale`, `--fail-on-unreviewed`, `--fail-on-req-changed` control which `Warning`-severity diagnostics promote to exit 1.
- Diagnostics are accumulated, sorted by file path then item name, then printed. No streaming output during pass 2.

**Size estimate:** ~100 lines.

---

### 10. `schema.rs` — Version Validation & Migration Scaffold

**Purpose:** Validate `"version"` field, scaffold future migration path.

**Behavior:**
- Accept `"0.1"` only. Return error for anything else.
- `migrate()` function: in 0.1, this is a no-op that returns the sidecar unchanged. The code path exists so that `--migrate` works from day one and future versions can add transformation logic.

**Size estimate:** ~20 lines.

---

## The Intents (Marker Types)

The convention defines 7 marker types that the linter recognizes in source files. Each has a specific semantic role:

### `@liyi:module`

- **Role:** Declares module-level intent — cross-function invariants.
- **Where:** Markdown files (`<!-- @liyi:module -->`), doc comments, source module preambles.
- **Linter behavior:** Checks for presence in directories that have `.liyi.jsonc` files (informational, not a failure). Does not parse or consume the prose content.
- **Examples:** "All monetary amounts carry their currency," "No endpoint accessible without authentication."

### `@liyi:requirement <name>`

- **Role:** Declares a named, prescriptive requirement — intent that exists before or alongside code.
- **Where:** Anywhere the linter walks — source comments, Markdown files, doc comments.
- **Linter behavior:** Discovered in pass 1 (project-global). Tracked in `.liyi.jsonc` with `"requirement"` key. Content is hashed for transitive staleness. Unique per repository.
- **Name syntax:** Parens for names with spaces (`@liyi:requirement(multi-currency addition)`), bare token otherwise (`@liyi:requirement auth-check`).

### `@liyi:related <name>`

- **Role:** Declares that a code item participates in a named requirement — creates a dependency edge.
- **Where:** In source, on the line(s) before the item definition.
- **Linter behavior:** Resolves the name to a tracked requirement in pass 2. Records the edge in `.liyi.jsonc` as `"related": {"<name>": "<hash>"}`. When the requirement's hash changes, all items with edges to it are flagged `REQ CHANGED`.

### `@liyi:intent [prose | =doc]`

- **Role:** Human assertion of intent for a code item — the explicit review path.
- **Where:** In source, on the line(s) before the item definition.
- **Linter behavior:** Detected within an item's `source_span`. Marks the item as reviewed. Takes precedence over sidecar `"reviewed": true` for adversarial testing. The `=doc` variant says "my docstring is my intent."

### `@liyi:trivial`

- **Role:** Classification — the item's intent is self-evident from its signature.
- **Where:** In source, on the line before the item definition.
- **Linter behavior:** No spec required. The item is reported as `· trivial`. Applied by the agent during inference for simple getters, setters, one-line wrappers.

### `@liyi:nontrivial`

- **Role:** Human override — "this looks trivial but I want a spec."
- **Where:** In source, on the line before the item definition.
- **Linter behavior:** Treats the item identically to an unannotated item — a spec is required. Prevents the agent from re-classifying as trivial.

### `@liyi:ignore`

- **Role:** Opt-out — the item is deliberately excluded from the convention.
- **Where:** In source, on the line before the item definition. Accepts optional reason after colon.
- **Linter behavior:** No spec required. The item is reported as `· ignored`. Used for internal helpers, legacy functions, FFI stubs.

---

## Key Constraints

### 1. No language-specific parsing
The linter reads line ranges and hashes bytes. It does not parse any programming language. Source markers are found by string matching on individual lines (after normalization). This is the core design constraint that makes the tool work with any language.

### 2. No LLM calls, no network access
The linter is fully offline and deterministic. SHA-256 hashing, file I/O, string matching. No API keys, no configuration for models, no telemetry.

### 3. No config file
Configuration is expressed through:
- CLI flags (`--fail-on-stale`, `--fail-on-unreviewed`, etc.)
- `.liyiignore` files (file-level exclusions)
- Inline annotations (`@liyi:ignore`, `@liyi:trivial`)
- No `.liyirc`, no `liyi.toml`, no `liyi.config.js`. Keep the surface area minimal.

### 4. Tool-managed vs. human-managed fields
| Field | Written by | Never written by |
|---|---|---|
| `item`, `intent`, `source_span`, `confidence`, `related` (names) | Agent | — |
| `source_hash`, `source_anchor`, `related` (hashes) | `liyi reanchor` / `liyi check --fix` | Agent, human |
| `reviewed` | Human (CLI / IDE) | Agent (security model) |

### 5. Exit code contract
| Exit code | Meaning |
|---|---|
| 0 | All checked specs are current. No failures per active flags. |
| 1 | At least one check failure (stale, unreviewed, req-changed, or error-severity diagnostic). |
| 2 | Internal error (malformed JSONC, unknown version). Supersedes exit 1. |

### 6. Sidecar naming convention
Always `<source_filename>.liyi.jsonc` — append to full filename, never strip extension. `money.rs` → `money.rs.liyi.jsonc`. This is enforced by discovery; ambiguous alternatives are warned.

### 7. 1-indexed, inclusive line numbers
All `source_span` values are 1-indexed closed intervals matching editor line numbers and `git blame` output. No internal conversion to 0-indexed.

---

## Remaining Work for 0.1 Release

### Must-have

#### R1. `liyi approve` — interactive review command (~2 hours)

The primary mechanism for transitioning intent from "agent-inferred" to "human-approved." Without this, "reviewable" is aspirational — users must hand-edit JSON to set `"reviewed": true`.

- Interactive by default when stdin is a TTY: show intent + source span, prompt `[y]es / [n]o / [e]dit / [s]kip`.
- Batch mode via `--yes` or when non-TTY.
- `--dry-run`, `--item <name>` flags.
- Reanchors on approval (fills `source_hash`, `source_anchor`).
- See design doc (`docs/liyi-design.md`) for full specification.

#### R2. `liyi init` — scaffold command (~1 hour)

Without this, bootstrapping requires hand-writing JSONC. Critical for first-run experience.

- `liyi init` — append agent instruction to `AGENTS.md`.
- `liyi init <source-file>` — create skeleton `.liyi.jsonc` sidecar.
- `--force` flag for overwriting existing files.

#### R3. Wire up remaining diagnostics (~1 hour)

1. **`RequirementCycle`**: In pass 2, when resolving `related` edges, track visited requirement names. If a cycle is detected, emit `RequirementCycle` diagnostic with the cycle path and stop traversing.
2. **`Untracked`**: After pass 1, for each requirement found in source markers that has no corresponding `Spec::Requirement` in any sidecar, emit `Untracked` diagnostic.
3. **`ReqNoRelated`**: After pass 2, for each requirement in the pass-1 map that no `Spec::Item` references via `related`, emit `ReqNoRelated` diagnostic (informational).
4. **`MalformedHash`**: During sidecar parsing or check, validate that `source_hash` values match `^sha256:[0-9a-f]+$`. Emit `MalformedHash` on mismatch.

#### R4. Missing golden-file fixtures (~30 min)

1. **`req_changed/`**: Source file with a `@liyi:requirement`, sidecar with a `Spec::Item` referencing it via `related` with a stale hash. Expect `ReqChanged` diagnostic, exit 1.
2. **`req_cycle/`**: Two requirements referencing each other transitively. Expect `RequirementCycle` diagnostic. (Depends on R3.)

#### R5. CI setup (~30 min)

1. GitHub Actions workflow: `cargo test` on push/PR.
2. Build `liyi` binary and run `liyi check --root .` as a CI step (dogfooding).
3. Cache `target/` for fast builds.

### Nice-to-have before 0.1

#### R6. Summary line output

Print a one-line summary after diagnostics: e.g., `12 current, 3 stale, 1 unreviewed`.

#### R7. Property-based tests for shift detection

`shift_proptest.rs`: generate random file content, insert/delete lines, verify shift detection correctness and delta propagation.

#### R8. `liyiignore/` golden fixture

A dedicated golden fixture for `.liyiignore` behavior (currently only tested by unit test).

---

## Dependency Table

| Crate | Version | Purpose | Size |
|---|---|---|---|
| `clap` | 4.x | CLI argument parsing (derive) | Well-known |
| `serde` + `serde_json` | 1.x | JSON (de)serialization | Well-known |
| `sha2` | 0.10.x | SHA-256 hashing | Minimal |
| `ignore` | 0.4.x | `.gitignore`/`.liyiignore`-aware directory walking | ripgrep's own crate |
| `proptest` | 1.x (dev) | Property-based testing for shift detection | Dev-only |

Optional:
| `json_comments` | 0.2.x | Strip JSONC comments before serde parsing | Tiny; can be hand-rolled (~20 lines) |

Total direct dependencies: 4 (runtime) + 1-2 (dev). Deliberately minimal.

---

## Testing Strategy

### Golden-file tests (`tests/fixtures/`)

Each fixture is a self-contained mini-repo:

| Fixture | Scenario | Expected output |
|---|---|---|
| `basic_pass/` | All specs reviewed and current | Exit 0, all `✓` |
| `stale_hash/` | Source changed, hash mismatch | Exit 1, `⚠ STALE` |
| `shifted_span/` | Lines shifted by N, content unchanged | Exit 0, `↕ SHIFTED` (auto-corrected) |
| `unreviewed/` | Specs exist but no review | Exit 0 (default) or 1 (`--fail-on-unreviewed`) |
| `orphaned_source/` | Source file deleted | Exit 1, `✗ source file not found` |
| `req_changed/` | Requirement text updated | Exit 1, `⚠ REQ CHANGED` |
| `req_cycle/` | Circular requirement hierarchy | Exit 1, `✗ requirement cycle detected` |
| `malformed_jsonc/` | Invalid JSON in sidecar | Exit 2, `✗ parse error` |
| `trivial_ignore/` | Items with `@liyi:trivial`/`@liyi:ignore` | Exit 0, `· trivial` / `· ignored` |
| `liyiignore/` | Files excluded by `.liyiignore` | Excluded files not reported |
| `span_past_eof/` | Span extends beyond file | Exit 1, `✗ source_span past EOF` |
| `fullwidth_markers/` | Markers with `＠` `：` `（` `）` | Recognized same as half-width |
| `multilingual_aliases/` | `@立意:忽略`, `@立意:需求`, etc. | Recognized same as English |

### Property-based tests

- Generate random file content (N lines).
- Insert/delete M lines at random positions.
- Verify shift detection finds the original content at the correct offset.
- Verify delta propagation correctly adjusts subsequent items.

### Dogfooding

The linter's own codebase has `.liyi.jsonc` specs. CI runs `liyi check`. This is both a test and a proof of concept.

---

## What's Explicitly Out of Scope for MVP

| Feature | Why deferred |
|---|---|
| LSP server | Depends on stable protocol; UX layer, not core |
| MCP server | Same — wrapper over CLI, not core |
| Challenge (`liyi challenge`) | Requires LLM integration; post-MVP |
| Adversarial test generation | Level 5; requires challenge + model integration |
| Tree-sitter-based span anchoring | Post-MVP upgrade for `source_span` resilience |
| `--smart` LLM-assisted staleness filter | Non-deterministic; developer-facing convenience |
| `liyi review` CLI subcommand | Post-MVP convenience; `"reviewed": true` can be set manually |
| Code-level dependency graph (`depends_on`) | Future direction for tighter staleness |
| Coverage detection (items without specs) | Requires item definition detection in source |
| `--require-ignore-reason` | Polish; not essential for 0.1 |
| `@liyi:end-requirement` closing marker | Future; `source_span` defines boundaries in 0.1 |
| Cross-repo intent sharing | Explicitly out of scope per design |
| Config file (`.liyirc`, `liyi.toml`) | No config file in 0.1; CLI flags + `.liyiignore` suffice |

---

## Success Criteria for MVP

1. **`liyi check` runs on a real codebase** — the linter's own source — and produces correct diagnostics. ✅ (29 unit + 12 integration tests pass)
2. **All golden-file tests pass** — covering every diagnostic in the catalog. 🟡 (10/12 planned fixtures exist; `req_changed/` and `req_cycle/` missing)
3. **`liyi reanchor` re-hashes spans** correctly, including `--item`/`--span` targeting. ✅
4. **The agent instruction works** — an LLM reading `AGENTS.md` produces valid `.liyi.jsonc` files that `liyi check` can lint. ✅
5. **CI is green** — GitHub Actions runs `liyi check` on every push. ❌ (not set up)
6. **The binary is small** — single static binary, <5 MB, zero runtime dependencies. ✅
7. **The README conveys the pitch** — a developer reading it for 60 seconds understands what 立意 does and how to try it. ✅

---

*立意 · MVP Implementation Roadmap · 2026-03-05 (updated 2026-03-06)*
