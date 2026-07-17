.PHONY: build test fmt fmt-check lint doc deny typos md ci hooks

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

doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

deny:
	cargo deny check

typos:
	typos

# Living docs only: docs/superpowers/ holds immutable execution artifacts
# (specs, plans) exempt from lint churn — see docs-style instructions.
md:
	npx --yes markdownlint-cli2 "**/*.md" "!_archive" "!target" "!node_modules" "!docs/superpowers" "!.superpowers"

ci: fmt-check lint test doc deny typos md
	@echo "✓ all CI checks green"

hooks:
	lefthook install
