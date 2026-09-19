<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Monorepo governance boundary — backlog

**Status:** Backlog. Recorded so the gap is not lost; not yet designed in
detail, and no requirement is declared yet.

## The gap

立意's recursive operations cannot tell directories a repository *owns*
from directories it merely *vendors*. In a layout such as

```text
repo/                  # owned
subprojects/*          # owned
3rdparty/*             # NOT owned
vendor/*               # NOT owned
```

`liyi migrate` (sidecar schema migration) and `liyi check` /
`liyi check --fix` walk the whole tree. They will therefore read and
rewrite artifacts under `3rdparty/` and `vendor/`, including a vendored
project's `.liyi.jsonc` sidecars and its `AGENTS.md` agent-instruction
block. Rewriting another project's intent artifacts to this repository's
schema/instruction revision is out of bounds.

## Current state

- **Agent instructions are already safe.** `liyi migrate` targeting
  strictly mirrors `liyi init`: a directory argument contributes exactly
  its own `AGENTS.md`, never a recursive walk, so a tree is traversed only
  by naming each governed directory. See *`liyi migrate` behavior* in
  `docs/liyi-design.md`.
- **Sidecars are not.** `resolve_sidecar_targets` still walks directories
  for `.liyi.jsonc`, so `liyi migrate .` can rewrite vendored sidecars.
- **`liyi check` is not.** It walks the whole tree (and `--fix` writes),
  so it reports and repairs vendored sidecars too.
- **Ignore files are the wrong shape.** `.liyiignore` / `.gitignore` can
  exclude paths, but the desired boundary is "which subtrees are under my
  governance", which is an *include* concept, not an ignore list — and a
  vendored tree may be committed, hence not gitignored.

## Open design questions

1. **What expresses the boundary?** Candidates:
   - an explicit *include* list of governed roots (CLI flag and/or a
     repo manifest), rather than an exclude list;
   - a marker at each governed root (a `.liyi/` directory or a
     file-scoped directive) so the boundary travels with the subtree;
   - a workspace manifest (`liyi.toml`?) at the repo root that enumerates
     or globs governed subtrees.
2. **Which operations become scoped?** Discovery, `check`, `check --fix`,
   `migrate`, `init`. Mutating paths matter most; read-only `check` is
   lower risk but still noisy across a vendored tree.
3. **Nested VCS boundaries.** The walker already skips VCS internals
   (`.git/`, `.hg/`, …), but a nested working tree or submodule is still
   walked. Should a nested `.git/` terminate the walk by default?
4. **Relationship to existing work.** How does this interact with the
   deferred "workspace-aware requirement queries (monorepo)" item and the
   shared requirement namespace?

## Minimum viable direction

An opt-in, explicit governed-root list (CLI flag and/or manifest) that
scopes discovery, `check`, `check --fix`, and `migrate` to the listed
subtrees, with the safe default remaining non-recursive for mutating
operations until that boundary exists.

## References

- `docs/liyi-design.md` — *`liyi migrate` behavior* (strict agent-file
  targeting), `.liyiignore`, and the sidecar discovery rules.
- `docs/next-steps.md` — Tier 5, "Workspace-aware requirement queries
  (monorepo)".
- Commit `3f451fcc38f7` — made `liyi migrate` agent-instruction targeting
  mirror `liyi init` (no recursion) for exactly this reason.
