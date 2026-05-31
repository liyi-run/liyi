<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Review Compression — Empirical Evidence

2026-05-31

---

## Summary

The design document (*Review scaling and complexity*) claims that the
review surface per item is "typically ~10% of the code surface." This
document presents an empirical measurement from the liyi codebase
itself — a self-referential data point where the tool's own code is
fully annotated with intent specs.

**Headline result: a reviewer reads ~515 lines of intent prose instead
of ~15,682 lines of source code — a 30.5× reduction in review
surface.** Pure intent prose is 3.3% of the source by line count.

But the aggregate hides a more interesting finding. Most code
*compresses* under review, but two categories *inflate*: GitHub Actions
workflows and `schema.rs` produce more intent prose than source. This
is counter-intuitive but important — see [Review inflation](#review-inflation-when-intent-exceeds-source).

---

## Methodology

The measurement script (`scripts/measure-intent-ratio.py`) walks the
repository and collects:

1. **Source files** — all production source files (`.rs`, plus the
   `.yml` GitHub Actions workflows and other supported languages),
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

## Results (2026-05-31 snapshot)

### Aggregate metrics

| Metric | Value |
|---|---|
| Production source files | 53 |
| Sidecar spec files | 49 |
| Source lines | 15,682 |
| Source bytes | 527,906 |
| Sidecar lines (full JSON) | 3,813 |
| Sidecar bytes | 171,592 |
| **Full sidecar / source (lines)** | **24.3%** |
| **Full sidecar / source (bytes)** | **32.5%** |

### Spec content breakdown

| Metric | Value |
|---|---|
| Total item specs | 173 |
| Prose intent | 148 |
| `=doc` sentinels | 17 |
| `=trivial` sentinels | 8 |
| Reviewed (human-approved) | 173 (100%) |
| Prose intent total characters | 41,194 |
| Average intent length | 278 characters (~3.5 lines) |
| Average source lines per spec | 90.6 |

### Review compression

| Metric | Value |
|---|---|
| Pure intent prose (est. lines at 80 ch/line) | ~515 |
| **Prose / source lines** | **3.3%** |
| **Prose / source bytes** | **7.8%** |
| **Reduction factor** | **30.5×** |

### Per-module breakdown

| Module | Source lines | Spec lines | Sidecar ratio |
|---|---|---|---|
| liyi (core) | 5,828 | 1,541 | 26.4% |
| liyi-cli | 1,250 | 296 | 23.7% |
| liyi::tree\_path | 7,183 | 1,209 | 16.8% |
| GHA workflows | 202 | 266 | 131.7% |

### Notable outliers

- **tree\_path/mod.rs** — 13.2% sidecar ratio (2,220 source lines, 294
  spec lines). The largest module compresses best because much of its
  volume is match-arm and grammar boilerplate covered by a few
  behavioral specs.
- **lang_go.rs** — 9.5% ratio (328 source lines, 31 spec lines), the
  best compressor: a language grammar adapter where one intent spec
  captures a long, mechanical structural mapping.
- **schema.rs** — 147.8% ratio (23 source lines, 34 spec lines).
  *Inflation*, not compression — see below.
- **GHA workflows** — 124.7% (`ci.yml`) and 135.7% (`release.yml`).
  Also inflation — see below.

---

## Interpretation

The design document claims "~10% of the code surface" as the per-item
review cost. This measurement shows two complementary results:

1. **Full sidecar ratio ≈ 24%** — the complete JSON sidecar (including
   structural metadata the reviewer doesn't need to read) is about
   one-quarter of the source. This is an upper bound.

2. **Pure intent prose ≈ 3.3%** — the actual natural-language text a
   reviewer reads is under 4% of the source. This is a lower bound on
   the review surface.

The ~10% figure in the design document falls between these two bounds,
which is consistent: it estimates the cost of reading intent prose plus
glancing at structural context (item names, spans) without reading the
full JSON metadata.

The 30.5× compression factor is specific to this codebase at this
snapshot. Contributors should expect variation across codebases with
different profiles:

- **Higher compression** in codebases with repetitive structure
  (language grammars, CRUD endpoints, generated adapters).
- **Lower compression** in codebases with dense, unique business logic
  where every function needs a detailed behavioral spec.

The `tree_path` module (16.8% sidecar ratio) demonstrates the
repetitive-structure case: 20 language-specific files share a structural
pattern, and one intent spec per language captures what would otherwise
be hundreds of lines of match arms.

---

## Review inflation: when intent exceeds source

The aggregate compression number is reassuring, but the per-file table
reveals two categories where intent prose is **longer** than the source
it describes:

| File | Source lines | Spec lines | Ratio |
|---|---|---|---|
| `.github/workflows/release.yml` | 129 | 175 | 135.7% |
| `.github/workflows/ci.yml` | 73 | 91 | 124.7% |
| `crates/liyi/src/schema.rs` | 23 | 34 | 147.8% |

This was initially dismissed as a trivial edge case ("the spec metadata
exceeds tiny source files"). On closer inspection it is something more
fundamental, and it inverts the usual intuition about where intent
specs pay off.

**Most code is a translation of a formal system.** A parser maps a
grammar to an AST; a hashing routine implements a named digest; a
match-heavy dispatch enumerates cases that are already spelled out in
the types. For this kind of code the source *is* the specification —
reading it tells you almost everything it should do. Intent prose only
has to capture the thin residue that the formalism leaves implicit, so
it compresses hard (the `tree_path` grammar adapters reach 9–16%).

**Contractual and ops-heavy code is the opposite.** Consider the CI
workflow. The source is terse, declarative YAML:

```yaml
permissions:
  contents: read
```

Those two lines carry an entire security argument that lives *nowhere*
in the source — the principle of least privilege, the fact that no job
in the workflow needs write access, and the consequence if that
assumption ever changes. The intent spec has to say all of it:

> Restrict the GITHUB_TOKEN to read-only content access (principle of
> least privilege). No job in this workflow needs write permissions.

The same pattern recurs across the workflow: "must skip tag refs so
release tags without DCO text do not fail CI," "`-D warnings` so any
lint fires a hard failure," "must not reformat in place — only check."
Each is a *why* or a *must-not* that the YAML cannot express. The
formal artifact is a configuration; the knowledge that makes it correct
is implicit, and intent specs are the only place it gets written down.

`schema.rs` inflates for the same reason. `validate_version` is six
lines of trivial string comparison, but its intent is a contract:
accept exactly `"0.1"`, reject everything else, and stay coupled to the
version field that the rest of the system depends on. The source shows
*what* it does in three seconds; the spec records *what it must keep
doing* and why — which is precisely the part a reviewer needs and the
part a refactor is most likely to break.

**The lesson inverts the naive cost model.** One might assume intent
specs are most valuable for large, complex functions and wasteful on
tiny declarative ones. The data says the opposite about *review value
per line*: the highest-inflation items are the smallest source files,
because their correctness depends almost entirely on implicit knowledge
— security posture, operational invariants, version contracts — that
the formal artifact deliberately omits. For this code, the sidecar is
not a compressed restatement of the source; it is the *only* written
record of the reasoning, and reviewing the prose is strictly more
informative than reviewing the YAML.

Inflation is therefore not a defect to be optimized away. It is a
signal that a file's real complexity lives outside its source text —
exactly the files where skipping the spec would be most dangerous.

---

## Threats to validity: this is a best case, not a typical one

The 30.5× figure is real, but it is measured on the most favorable
possible subject. liyi's own codebase is not a representative sample of
the repositories that would adopt the convention — it is the *ceiling*
the convention can reach when intent exists before the code. Three
confounds make it an outlier, and each one runs in the optimistic
direction.

**1. Reverse causality — the code was compiled from the intent.** This
codebase was bootstrapped from a polished design document; much of the
source is, in effect, an LLM "compilation" of prose that already
existed. The intent specs therefore do not *summarize* the code — the
code is a *projection of* the intent. Specs compress cleanly because
they sit upstream of the source, not because they were archaeologically
recovered from it. A brownfield project inverts this entirely: the code
is ground truth, and intent must be reconstructed after the fact, often
when the original reasoning was never written down and its author is
long gone. The inflation files (CI permissions, the version contract)
are precisely where that original rationale is *least* likely to have
survived in anyone's memory — so the brownfield versions of those specs
will be slower to write, longer, and lower-confidence.

**2. Greenfield 100%-reviewed is a luxury.** The breakdown shows
173/173 specs reviewed — full human approval. That is achievable only
because the specs were authored as the code was written, by someone who
held the design in their head. A brownfield repository's realistic
steady state is a small reviewed core surrounded by a large
un-reviewed (inferred-or-nothing) periphery. The clean, short average
intent (278 characters) reflects an author who *knew* each invariant;
the brownfield equivalent — a maintainer guessing at the intent of a
years-old function whose author has left — produces longer, hedged,
lower-confidence prose.

**3. Bootstrapping cost is excluded entirely.** This measurement
captures the value of the intent ledger *once it exists*. It says
nothing about the cost of building one. For greenfield that cost is
near zero because intent is written alongside code; for brownfield it
is the dominant cost — and it is the entire concern of the audience
most often cited for this tooling (burned-out maintainers triaging
AI-generated contributions). The compression payoff is real but
*deferred*: it accrues only after enough inference-and-promotion cycles
have built a reviewed core covering the hot paths. The greenfield
number cannot be borrowed to advertise the brownfield experience,
because greenfield skipped the very cost that dominates brownfield —
writing the first doc.

**What this means.** This document measures the value of the ledger
once it exists; it does not measure the cost of bootstrapping one, and
bootstrapping cost is the whole question for an existing project. The
honest claim is narrow: *when intent is captured before or alongside
code, the review surface compresses by roughly 30× on this codebase.*
The experiment that would speak to brownfield adoption is different in
kind — take a real repository with no intent docs, run LLM inference
plus incremental human promotion across many pull requests, and measure
how many cycles pass before the reviewed core covers the hot paths and
compression begins to bite. That number is not in this document, and it
should not be inferred from the one that is.

---

## Reproducing

```sh
python3 scripts/measure-intent-ratio.py [ROOT]
```

Run from the repository root (or pass `.` as ROOT). The script requires
only Python 3.10+ and the standard library.
