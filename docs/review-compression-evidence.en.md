<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Review Compression — Empirical Evidence

2026-03-14

---

## Summary

The design document (*Review scaling and complexity*) claims that the
review surface per item is "typically ~10% of the code surface." This
document presents the first empirical measurement from the liyi codebase
itself — a self-referential data point where the tool's own code is
fully annotated with intent specs.

**Headline result: a reviewer reads ~394 lines of intent prose instead
of ~10,488 lines of source code — a 26.6× reduction in review
surface.** Pure intent prose is 3.8% of the source by line count.

---

## Methodology

The measurement script (`scripts/measure-intent-ratio.py`) walks the
repository and collects:

1. **Source files** — all production `.rs` files under `crates/`,
   excluding `tests/` and `fixtures/` directories.
2. **Sidecar files** — all `.liyi.jsonc` files co-located with the
   source files above.
3. **Spec content** — parsed from the JSONC sidecars: item name,
   `intent` text, `=doc` / `=trivial` sentinels, `reviewed` status.

Three levels of compression are reported:

| Level | What it measures | Includes |
|---|---|---|
| **Full sidecar** (line/byte ratio) | Total spec file size vs source | JSON boilerplate, hashes, spans, intent prose |
| **Byte ratio** | Raw bytes of sidecar vs source | Same as above, by bytes |
| **Pure intent prose** | Just the natural-language intent strings | Only the `"intent"` field values, no JSON structure |

The pure intent prose level is what a human reviewer actually reads.
The JSON metadata (`source_span`, `source_hash`, `tree_path`, etc.)
is consumed by the linter, not by the reviewer.

---

## Results (2026-03-14 snapshot)

### Aggregate metrics

| Metric | Value |
|---|---|
| Production source files | 39 |
| Sidecar spec files | 38 |
| Source lines | 10,488 |
| Source bytes | 368,695 |
| Sidecar lines (full JSON) | 2,012 |
| Sidecar bytes | 97,051 |
| **Full sidecar / source (lines)** | **19.2%** |
| **Full sidecar / source (bytes)** | **26.3%** |

### Spec content breakdown

| Metric | Value |
|---|---|
| Total item specs | 106 |
| Prose intent | 102 |
| `=doc` sentinels | 4 |
| `=trivial` sentinels | 0 |
| Reviewed (human-approved) | 106 (100%) |
| Prose intent total characters | 31,500 |
| Average intent length | 309 characters (~4 lines) |
| Average source lines per spec | 98.9 |

### Review compression

| Metric | Value |
|---|---|
| Pure intent prose (est. lines at 80 ch/line) | ~394 |
| **Prose / source lines** | **3.8%** |
| **Prose / source bytes** | **8.5%** |
| **Reduction factor** | **26.6×** |

### Per-module breakdown

| Module | Source lines | Spec lines | Sidecar ratio |
|---|---|---|---|
| liyi (core) | 4,280 | 939 | 21.9% |
| liyi-cli | 985 | 248 | 25.2% |
| liyi::tree\_path | 5,209 | 825 | 15.8% |

### Notable outliers

- **check.rs** — 4.4% sidecar ratio (1,555 source lines, 68 spec
  lines). The largest module compresses best because much of its
  volume is match-arm boilerplate covered by a few behavioral specs.
- **schema.rs** — 147.8% ratio (23 source lines, 34 spec lines). A
  trivial re-export where the spec metadata exceeds the source — an
  expected edge case.

---

## Interpretation

The design document claims "~10% of the code surface" as the per-item
review cost. This measurement shows two complementary results:

1. **Full sidecar ratio ≈ 19%** — the complete JSON sidecar (including
   structural metadata the reviewer doesn't need to read) is about
   one-fifth of the source. This is an upper bound.

2. **Pure intent prose ≈ 3.8%** — the actual natural-language text a
   reviewer reads is under 4% of the source. This is a lower bound on
   the review surface.

The ~10% figure in the design document falls between these two bounds,
which is consistent: it estimates the cost of reading intent prose plus
glancing at structural context (item names, spans) without reading the
full JSON metadata.

The 26.6× compression factor is specific to this codebase at this
snapshot. Contributors should expect variation across codebases with
different profiles:

- **Higher compression** in codebases with repetitive structure
  (language grammars, CRUD endpoints, generated adapters).
- **Lower compression** in codebases with dense, unique business logic
  where every function needs a detailed behavioral spec.

The `tree_path` module (15.8% sidecar ratio) demonstrates the
repetitive-structure case: 20 language-specific files share a structural
pattern, and one intent spec per language captures what would otherwise
be hundreds of lines of match arms.

---

## Reproducing

```sh
python3 scripts/measure-intent-ratio.py [ROOT]
```

Run from the repository root (or pass `.` as ROOT). The script requires
only Python 3.10+ and the standard library.
