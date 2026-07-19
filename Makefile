.PHONY: build test fmt fmt-check lint doc deny typos md machete ci hooks tla

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

# Catches unused *dependencies* (neither clippy nor `cargo deny` do) — see
# phase-2-carryover-hardening AC3. Not auto-installed (unlike `make tla`'s
# jar download) because `cargo install` recompiles a whole tool on a cache
# miss, which is too slow to hide inside a target run on every `make ci`;
# install once with: cargo install cargo-machete --version 0.9.2 --locked
# (CI's `unused-deps` job pins the same version — keep both in sync).
machete:
	@command -v cargo-machete >/dev/null 2>&1 || { \
		echo "cargo-machete not found — install with: cargo install cargo-machete --version 0.9.2 --locked"; \
		exit 1; \
	}
	cargo machete

ci: fmt-check lint test doc deny typos md machete
	@echo "✓ all CI checks green"

hooks:
	lefthook install

TLA_TOOLS := docs/formal/.tools/tla2tools.jar

$(TLA_TOOLS):
	mkdir -p docs/formal/.tools
	curl -fsSL https://github.com/tlaplus/tlaplus/releases/latest/download/tla2tools.jar -o $(TLA_TOOLS)

tla: $(TLA_TOOLS)
	cd docs/formal && java -XX:+UseParallelGC -jar .tools/tla2tools.jar -config lease_protocol.cfg lease_protocol.tla
