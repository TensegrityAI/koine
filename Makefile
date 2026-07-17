.PHONY: build test fmt fmt-check lint deny typos ci hooks

build:
	cargo build --workspace

test:
	cargo test --workspace

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all --check

lint:
	cargo clippy --workspace --all-targets -- -D warnings

deny:
	cargo deny check

typos:
	typos

ci: fmt-check lint test deny typos
	@echo "✓ all CI checks green"

hooks:
	lefthook install
