<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
<!-- @liyi:module -->

# 《立意》Lìyì — *Establish intent before execution*

🌍: [简体中文（普通话）](README.zh.md) / English

> ⚠️ **Work in progress** — the spec format and CLI are under active evolution. Expect breaking changes.
>
> 🐕 **Bootstrapped — intent before execution:** we wrote the design doc first, then agents bootstrapped the entire project from it, following the full 立意 pattern before the first line of code. **Specs track code:** when source or requirements change, `liyi check` detects staleness and the agent resolves it automatically — code and intent stay in sync.

**立意** is a convention and CLI tool that makes intent explicit, persistent, and reviewable in AI-assisted software development. It pairs every code item with a human-readable statement of what the item *should* do, stored in language-agnostic sidecar files (`.liyi.jsonc`). A CI linter (`liyi check`) detects when source changes outpace intent — catching staleness, orphaned specs, and broken requirement edges — so that the gap between "what the AI wrote" and "what the human intended" never grows silently.

## Quick Start

```bash
# Install (from source)
cargo install --path crates/liyi-cli

# Have an agent generate intent specs (writes .liyi.jsonc files)
# You can just tell it to — no special command, the agent is the command

# Then fill in hashes:
liyi check --fix --root .

# Run manually, or wire into CI:
liyi check --root .
```

## How It Works

1. **Agent infers intent** — today's agents automatically read `AGENTS.md`, which teaches them the 立意 pattern. During normal development they maintain `.liyi.jsonc` sidecar files for each code item, with `source_span` and natural-language `intent`. If they don't do it automatically, you can always tell them to.
2. **`liyi check`** — hashes source spans, detects staleness and shifts, checks review status, tracks requirement edges. Zero network, zero LLM, fully deterministic. With `--fix`, auto-corrects shifted spans, fills missing hashes, and computes `tree_path`.
3. **`liyi migrate`** — upgrades sidecar files when the schema version changes. Idempotent.
4. **Human reviews** — sets `"reviewed": true` in the sidecar to approve, or adds `@liyi:intent` in source to provide the authoritative human version.

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

liyi migrate [FILE|DIR]...
    Upgrade sidecar schema version
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | All specs current, no failures |
| 1 | Check failure (stale, unreviewed, req-changed) |
| 2 | Internal error (malformed JSONC, unknown schema version) |

## License

`SPDX-License-Identifier: Apache-2.0 OR MIT`
