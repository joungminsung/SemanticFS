.PHONY: setup build test lint fmt clean install

# One-step setup: installs Rust, FUSE, builds, tests
setup:
	@./scripts/setup-dev.sh

build:
	cargo build --workspace

build-release:
	cargo build --workspace --release

test:
	cargo test --workspace

lint:
	cargo clippy --workspace -- -D warnings

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all --check

clean:
	cargo clean

install:
	cargo install --path crates/semfs-cli

# Run semfs commands
index:
	cargo run --bin semfs -- index $(SOURCE)

search:
	cargo run --bin semfs -- search "$(QUERY)"

status:
	cargo run --bin semfs -- status

diagnose:
	cargo run --bin semfs -- diagnose --json

# CI checks (same as GitHub Actions)
ci: fmt-check lint test
	@echo "All CI checks passed."
