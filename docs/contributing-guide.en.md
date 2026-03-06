<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Contributing Guide for AI Agents

This document is the English version of the maintainer-preferred guide for AI agents working on this repository. For the Chinese version, see [contributing-guide.zh.md](contributing-guide.zh.md).

## Key expectations

- Keep changes minimal and scoped.
- One logical change per commit (no unrelated edits in the same commit).
- Avoid reformatting unrelated files.
- Review diffs for unrelated changes before finalizing.
- Update `AGENTS.md` when architectural changes occur.

## AIGC policy

This project enforces a strict AI-Generated Content (AIGC) policy. **All AI agents must read and comply with the full policy before contributing.**

- English: [aigc-policy.en.md](aigc-policy.en.md)
- 中文: [aigc-policy.zh.md](aigc-policy.zh.md)

Key points (the full policy is authoritative):

- **Commit separation**: Each commit should be either entirely human-written or entirely AI-written. Do not mix.
- **Identity disclosure**: AI agents must add an `AI-assisted-by` trailer to every AIGC commit (e.g., `AI-assisted-by: Claude Opus 4.6 (GitHub Copilot)`).
- **Original prompt**: Record the original user prompt in the commit message body before the trailers.
- **Human review**: All commits must be reviewed by a human, who appends `Signed-off-by` (DCO). AI agents must **not** add this tag on behalf of the user.
- **No sensitive information**: Never include keys, credentials, or personal data in commits.

## Project overview

- **Purpose**: 立意 (Lìyì) defines a convention and ships tooling for making intent explicit, persistent, and reviewable in AI-assisted software development. It is *not* an agent skill collection — it produces the practice, the linter, the spec format, and an agent skill template that other projects adopt.
- **Languages**: Markdown (design docs, templates), with implementation code expected in Rust or similar as the project matures.
- **License**: `Apache-2.0 OR MIT`

High-level layout:

- `docs/`: Design documents, AIGC policy, contributing guides, and other project-level documentation.
- `AGENTS.md`: Agent instructions for this repository.
- `README.md`: Project introduction.

As the project grows, expect additional top-level directories for implementation (linter, CLI) and the deliverable agent skill template.

## Content workflows

### Design documents

- The design document (`docs/liyi-design.md`) is the authoritative specification. Changes to it should be deliberate and well-motivated.
- When proposing design changes, explain the rationale in the commit body.

### Agent skill template

- The agent skill template is a deliverable of this project — a set of files (AGENTS.md, contributing guides, AIGC policy) that downstream repositories can adopt and customize.
- When editing template content, consider both this project's own use and the downstream adoption experience. Template files should be clearly generalizable.

### Documentation

- Project-level docs live in `docs/`.
- The AIGC policy documents (`docs/aigc-policy.*.md`) are normative — do not weaken or contradict them elsewhere.

### Bilingual requirement

- All documentation must be provided in **English and Chinese** to maximize outreach.
- When creating or modifying a document, always sync the change to the other language version in the same commit or an immediately following commit.
- Code identifiers and commit messages remain in English.

## Code style and conventions

- Markdown files should use ATX-style headings (`#`, `##`, etc.).
- Use blank lines after headings and before code blocks.
- Keep lines at a reasonable length (wrap around 80–100 characters where practical).
- SPDX license headers (`<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->`) should appear at the top of documentation files.

### Quine-escape convention

The linter scans source files for `@liyi:*` markers using plain substring matching — it has no language awareness. This means string constants, format strings, and test data that contain literal marker text will be misidentified as real markers (the classic "quine" problem of a program reading its own source).

To prevent self-triggering, **escape the `@` character** in any string literal that spells out a marker:

| Language | Escape | Example |
|---|---|---|
| Rust | `\x40` | `"\x40liyi:ignore"` |
| JSON | `\u0040` | `"\u0040liyi:requirement"` |

Actual marker comments (e.g. `// @liyi:intent =doc`) must keep the literal `@` — they are real markers.

This invariant is enforced as `@liyi:requirement(quine-escape)` in `src/markers.rs`.

### Chinese writing style

When writing Chinese content, avoid **topic-comment sentence structures**. Use **subject-predicate constructions** with appropriate prepositions or particles instead, so that readers of all backgrounds can understand the text more easily.

Specific rules:

- **Do not let a bare topic serve as the grammatical subject while omitting the preposition.** Use prepositions such as "对于", "为", or "在" to make the semantic relationship explicit.
- **Do not omit the passive-voice marker for inanimate subjects** ("被", "由", etc.), or rewrite as an explicit active-voice sentence with the otherwise implied object spelled out.

Examples:

| ❌ Topic-comment (avoid) | ✅ Subject-predicate (preferred) |
|---|---|
| 设计文档描述了规范 | 在设计文档中描述了规范 |
| Markdown 文件使用 ATX 风格标题 | 为 Markdown 文件使用 ATX 风格标题 |
| 所有文件均须提供双语言版本 | 必须为所有文件提供双语言版本 |

## Commit message style

Follow Conventional Commits:

```plain
<type>(<scope>): <summary>
```

Guidelines:

- Imperative, present-tense summary (no trailing period).
- ~50–72 characters for summary.
- One logical change per commit — do not combine unrelated changes.
- Include a body when needed to explain motivation or key changes.
- Separate body from summary with a blank line; wrap body lines around 72 characters.

Types:

- `feat`: New feature or capability
- `fix`: Bug fix or correction
- `docs`: Changes to documentation (design docs, guides, policies)
- `refactor`: Restructuring without behavior change
- `build`: Build system or dependency changes
- `ci`: CI/CD configuration changes

Do **not** use `chore` — use `build` or `ci` as appropriate instead.

Scopes:

- `design`: Design document (`docs/liyi-design.md`)
- `linter`: Linter implementation
- `template`: Agent skill template deliverable
- `docs`: General documentation
- `policy`: AIGC policy
- `meta`: Repository metadata (README, AGENTS.md, licenses)

### AIGC commit requirements

Per the [AIGC policy](aigc-policy.en.md), AI agents must:

1. Disclose identity with an `AI-assisted-by` trailer.
2. Record the original prompt in the commit body.
3. **Not** add `Signed-off-by` on behalf of the user.

Example:

```plain
feat(linter): implement source hash comparison

Add source hash computation and comparison logic for detecting
stale intent specs.

Original prompt:

> Implement the source_hash staleness check described in the
> design document section on linter behavior.

AI-assisted-by: Claude Opus 4.6 (GitHub Copilot)
Signed-off-by: Contributor Name <contributor@example.com>
```

## Validation checklist

Before finalizing a commit, verify:

- ✅ Markdown files are well-formed (no broken links, proper heading hierarchy).
- ✅ SPDX license headers are present on new documentation files.
- ✅ Both English and Chinese versions are created or updated in sync.
- ✅ Commit message follows Conventional Commits and AIGC policy requirements.
- ✅ No sensitive information is included.
- ✅ Changes are scoped to a single logical unit.
