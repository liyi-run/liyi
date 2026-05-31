# SPDX-License-Identifier: Apache-2.0 OR MIT

.PHONY: lint fmt-check clippy liyi-check test

# Run every pre-commit lint gate (matches CI).
lint: fmt-check clippy liyi-check

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace -- -D warnings

# Dogfood the project's own linter on this repository.
liyi-check:
	cargo run -p liyi-cli -- check --root .

test:
	cargo test --workspace
