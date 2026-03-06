<!-- @liyi:module -->

# 《立意》Lìyì — *Establish intent before execution*

**立意** is a convention and CLI tool that makes intent explicit, persistent, and reviewable in AI-assisted software development. It pairs every code item with a human-readable statement of what the item *should* do, stored in language-agnostic sidecar files (`.liyi.jsonc`). A CI linter (`liyi check`) detects when source changes outpace intent — catching staleness, orphaned specs, and broken requirement edges — so that the gap between "what the AI wrote" and "what the human intended" never grows silently.

## Quick Start

```bash
# Install (from source)
cargo install --path .

# Have an agent generate intent specs (writes .liyi.jsonc files)
# Then fill in hashes:
liyi check --fix --root .

# Run the linter in CI:
liyi check --root .
```

## How It Works

1. **Agent infers intent** — reads `AGENTS.md`, writes `.liyi.jsonc` sidecar files with `source_span` and natural-language `intent` for each code item.
2. **`liyi check`** — hashes source spans, detects staleness and shifts, checks review status, tracks requirement edges. Zero network, zero LLM, fully deterministic.
3. **`liyi reanchor`** — re-hashes spans after intentional code changes. Never modifies intent or review state.
4. **Human reviews** — sets `"reviewed": true` or adds `@liyi:intent` in source to approve.

## Progressive Adoption

| Level | What you do | What you get |
|-------|-------------|--------------|
| 0 | Copy `AGENTS.md` paragraph into your repo | Agent writes `.liyi.jsonc` instead of nothing |
| 1 | Add `liyi check` to CI | Staleness detection on every push |
| 2 | Review intent, set `reviewed: true` | Guaranteed human-in-the-loop on meaning |
| 3 | Add `@liyi:requirement` markers | Cross-cutting concerns tracked transitively |
| 4 | Use `@liyi:intent` in source | Intent lives next to code, survives refactors |
| 5 | Generate adversarial tests from reviewed intent | Tests that catch subtle semantic drift |

## CLI Reference

```
liyi check [OPTIONS] [PATHS]...
    --fix                           Auto-correct shifted spans, fill missing hashes
    --fail-on-stale <true|false>    Fail on stale specs (default: true)
    --fail-on-unreviewed <true|false>  Fail on unreviewed specs (default: false)
    --fail-on-req-changed <true|false> Fail on changed requirements (default: true)
    --root <PATH>                   Override repo root

liyi reanchor [FILE]
    --item <NAME>     Target a specific item
    --span <S,E>      Override span (1-indexed, inclusive)
    --migrate         Schema version migration
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | All specs current, no failures |
| 1 | Check failure (stale, unreviewed, req-changed) |
| 2 | Internal error (malformed JSONC, unknown schema version) |

## License

`SPDX-License-Identifier: Apache-2.0 OR MIT`
