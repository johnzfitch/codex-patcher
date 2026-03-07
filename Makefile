.PHONY: test test-args test-verbose check lint fmt

# Compact: one line per binary, failures named explicitly
test:
	@scripts/test

# Pass extra args:  make test-args ARGS="--test integration"
test-args:
	@scripts/test $(ARGS)

# Full cargo output
test-verbose:
	cargo test --all-targets

check:
	cargo check --all-targets

lint:
	cargo clippy --all-targets -- -D warnings

fmt:
	cargo fmt
