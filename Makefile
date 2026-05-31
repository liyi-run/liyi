# SPDX-License-Identifier: Apache-2.0 OR MIT

.PHONY: verify lint fmt-check clippy liyi-check test

# Run every pre-commit gate (lint + tests) in one shot.
#
# Make aborts at the first failing gate, so reaching the final
# "verify: all gates passed" line proves every gate succeeded. There is no
# need to inspect `$?` or redirect output to scan for an exit code: a clean
# run ends with the success banner, and a failing run stops with a non-zero
# make exit plus the offending gate's error.
verify: lint test
	@echo "verify: all gates passed"

# Run every pre-commit lint gate (matches CI). Ends with a success banner.
lint: fmt-check clippy liyi-check
	@echo "lint: all gates passed"

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace -- -D warnings

# Dogfood the project's own linter on this repository.
liyi-check:
	cargo run -p liyi-cli -- check --root .

test:
	cargo test --workspace
	@echo "test: passed"
