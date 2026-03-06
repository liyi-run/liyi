<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Contributing

Thank you for your interest in 立意 (Lìyì)!

> **⚠️ Work in progress** — This project is in early, active development and
> maintained **part-time by a solo developer**. Expect very limited bandwidth
> for reviewing contributions, except for good-quality content, urgent bug
> fixes and security issues.

## Before You Start

Detailed contributing guidelines (for both humans and AI agents) live in the
`docs/` directory:

- **English** → [docs/contributing-guide.en.md](docs/contributing-guide.en.md)
- **中文** → [docs/contributing-guide.zh.md](docs/contributing-guide.zh.md)

Please read the appropriate guide before submitting a PR. It covers project
structure, code style, commit conventions, and the AIGC policy.

## Quick Checklist

1. **Open an issue first** for non-trivial changes so we can discuss scope.
2. Keep PRs small and focused — one logical change per PR.
3. Ensure `cargo test --workspace` passes.
4. Ensure `cargo clippy --workspace -- -D warnings` is clean.
5. Ensure `cargo run -p liyi-cli -- check --root .` passes (the project
   dogfoods its own linter).
6. Follow the [AIGC policy](docs/aigc-policy.en.md) if using AI assistance.

## License

By contributing, you agree that your contributions will be licensed under
`Apache-2.0 OR MIT`, consistent with the project's existing license.
