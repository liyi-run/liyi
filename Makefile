# SPDX-License-Identifier: Apache-2.0 OR MIT

.PHONY: verify lint fmt-check clippy liyi-check test \
        verify-short lint-short test-short

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

# --- Quiet variants for token-constrained AI agents ----------------------
#
# Agents often pipe `make lint` into a log and grep for the exit code to avoid
# flooding their context with the linter's full item listing. These targets
# do that for you: they run the SAME gates as their full counterparts but
# suppress normal output. On success each prints only a one-line banner. On
# failure each prints the tail of the offending gate's output and tells you to
# re-run the full `make <gate>` target for the complete diagnostics.
#
# Human contributors should prefer the full `make verify` / `make lint`; the
# short variants exist to conserve agent context, not to hide information.

# Quiet lint. Mirrors `lint`; re-run `make lint` on failure for full output.
lint-short:
	@LOG=$$(mktemp); \
	if ! cargo fmt --all -- --check >$$LOG 2>&1; then \
		tail -n 20 $$LOG; rm -f $$LOG; \
		echo "lint-short: rustfmt FAILED — re-run \`make fmt-check\` for the full diff"; exit 1; \
	fi; \
	if ! cargo clippy --workspace --quiet -- -D warnings >$$LOG 2>&1; then \
		tail -n 20 $$LOG; rm -f $$LOG; \
		echo "lint-short: clippy FAILED — re-run \`make clippy\` for the full output"; exit 1; \
	fi; \
	if ! cargo run --quiet -p liyi-cli -- check --root . >$$LOG 2>&1; then \
		tail -n 20 $$LOG; rm -f $$LOG; \
		echo "lint-short: liyi check FAILED — re-run \`make liyi-check\` for the full report"; exit 1; \
	fi; \
	rm -f $$LOG; \
	echo "lint-short: all gates passed"

# Quiet tests. Mirrors `test`; re-run `make test` on failure for full output.
test-short:
	@LOG=$$(mktemp); \
	if ! cargo test --workspace --quiet >$$LOG 2>&1; then \
		tail -n 30 $$LOG; rm -f $$LOG; \
		echo "test-short: tests FAILED — re-run \`make test\` for the full output"; exit 1; \
	fi; \
	rm -f $$LOG; \
	echo "test-short: passed"

# Quiet lint + tests. Re-run `make verify` on failure for full output.
verify-short: lint-short test-short
	@echo "verify-short: all gates passed"
