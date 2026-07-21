# Koiné Phase 2A Operational Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close every remaining phase-0/1/2A supply-chain, packaging, manifest, and documentation gap, then prove the complete phase-2A product gate before unblocking phase-2B planning.

**Architecture:** Turn artifact identity into an enforced repository policy, make protobuf generation hermetic, make every crate explicitly non-publishable until phase 2B, and reconcile all public/governance documents with executable reality. Finish with fresh formal, CI, packaging, real Postgres, and real gRPC evidence plus independent review.

**Tech Stack:** GitHub Actions, Bash/Make, SHA-256, Cargo/Cargo.lock, `protoc-bin-vendored` 3.2.0, npm lockfile, Markdown, Docker/Postgres, existing Rust/TLA+ test suites.

## Global Constraints

- Implements approved ADR-0017 and hardening design §§6–10.
- Execute only after atomic-lease and Postgres-resource plans are closed.
- No `cargo publish`, push, release, SDK, REST, MCP, dashboard, benchmark, or HA work.
- Every crate is explicitly `publish = false` until a phase-2B publication decision.
- External dependency edges must not change except approved hermetic/tooling dependencies.
- Public docs describe implemented behavior; future capabilities carry their phase label.
- Current pins below were resolved from official upstream APIs/releases on 2026-07-21.

---

## Verified immutable inputs

| Input | Version | Immutable identity |
| --- | --- | --- |
| `actions/checkout` | v7.0.1 | `3d3c42e5aac5ba805825da76410c181273ba90b1` |
| `actions/setup-java` | v5.6.0 | `03ad4de0992f5dab5e18fcb136590ce7c4a0ac95` |
| `actions-rust-lang/setup-rust-toolchain` | v1.17.0 | `166cdcfd11aee3cb47222f9ddb555ce30ddb9659` |
| `EmbarkStudios/cargo-deny-action` | v2.1.1 | `3c6349835b2b7b196a839186cb8b78e02f7b5f25` |
| `crate-ci/typos` | v1.48.0 | `bee27e3a4fd1ea2111cf90ab89cd076c870fce14` |
| TLA+ tools | v1.7.4 | SHA-256 `936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88` |
| Temurin JDK | 21.0.11+10 | exact `setup-java` input |
| `markdownlint-cli2` | 0.22.1 | exact npm lock; Node `>=20` |
| `protoc-bin-vendored` | 3.2.0 | exact Cargo requirement and lock checksums |
| PostgreSQL image | 17 | `sha256:a426e44bac0b759c95894d68e1a0ac03ecc20b619f498a91aae373bf06d8508d` |

## File map

- `.apptlas/policies/supply-chain-policy.md`: immutable-input policy/exceptions.
- `.github/scripts/check-supply-chain.sh`: executable drift gate.
- `.github/workflows/ci.yml`, `Makefile`, `package.json`, `package-lock.json`: pinned CI/tooling.
- `crates/koine-proto/{Cargo.toml,build.rs}` and `Cargo.lock`: hermetic protobuf compiler.
- `Cargo.toml`, every `crates/*/Cargo.toml`: internal workspace dependencies, descriptions, publish intent.
- `compose.yaml` and test support: immutable Postgres image.
- `README.md`, `ROADMAP.md`, `CLAUDE.md`, `.env.example`, architecture wiki, phase-2 epic: truthful live state.
- Three legacy todo items plus new operational closure item: final evidence.

### Task 1: Open the operational closure item

**Files:**

- Create then move: `.apptlas/backlog/{todo,ongoing}/phase-2a-operational-closure.md`

**Interfaces:**

- Consumes: design §§6–10, ADR-0017, prior two closed hardening items.
- Produces: owner of final phase-unblock decision.

- [ ] **Step 1: Create the ready item**

```markdown
# Close phase-2A operational and supply-chain debt

- **State:** todo
- **Origin:** phase-2A zero-debt hardening design
- **Epic:** ../epics/phase-2-data-plane.md

## Traceability

- **Implements:** hardening design §§6–10; ADR-0017; operational-closure plan Tasks 1–6; closes the CI-pinning and manifest-cleanup legacy items.

## Acceptance criteria

- [ ] AC1: repository-owned actions/downloads/tooling satisfy the accepted immutable-input policy and an automated gate rejects floating regressions — *verify:* `make supply-chain` plus deliberate mutation probe.
- [ ] AC2: protobuf builds with a deliberately invalid `PROTOC`, proving vendored compiler selection — *verify:* isolated-target `cargo build -p koine-proto`.
- [ ] AC3: internal dependency edges are identical before/after centralization; descriptions contain no backticks and every crate is non-publishable — *verify:* normalized metadata diff and manifest scan.
- [ ] AC4: every implemented crate's package file list contains required sources/assets/licenses — *verify:* `cargo package --allow-dirty --list -p ...`.
- [ ] AC5: README, roadmap, living context, epic, env reference, architecture wiki, and backlog agree about present/future behavior — *verify:* docs/spec review.
- [ ] AC6: formal, full CI, real Postgres dev-loop, real TCP/Postgres gRPC, server startup/shutdown, and zero-debt audit are fresh green — *verify:* final gate commands.

## Dependencies

- Atomic lease and Postgres resource hardening items closed with review.

## Evidence (filled at close)

## Spec-fidelity statement (filled at close)
```

- [ ] **Step 2: Move and commit**

```bash
git mv .apptlas/backlog/todo/phase-2a-operational-closure.md .apptlas/backlog/ongoing/
git add .apptlas/backlog/ongoing/phase-2a-operational-closure.md
git commit -m "docs: open phase 2a operational closure"
```

### Task 2: Enforce immutable CI and downloaded artifacts

**Files:**

- Create: `.apptlas/policies/supply-chain-policy.md`
- Create: `.github/scripts/check-supply-chain.sh`
- Create: `package.json`, `package-lock.json`
- Modify: `.github/workflows/ci.yml`, `Makefile`, `.apptlas/policies/README.md` if present.

**Interfaces:**

- Produces: `make supply-chain`; exact CI action/JDK/TLA/Markdownlint pins.
- Consumed by: final `make ci` and Task 6.

- [ ] **Step 1: Write the failing policy gate**

Create an executable Bash script that scans only `.github/workflows`, `Makefile`, `compose.yaml`, and repository-owned scripts. Its action check must implement:

```bash
status=0
while IFS= read -r match; do
  target=${match#*uses: }
  target=${target%%#*}
  target=${target//[[:space:]]/}
  if [[ "$target" != ./* && ! "$target" =~ @[0-9a-f]{40}$ ]]; then
    echo "floating GitHub Action: $match" >&2
    status=1
  fi
done < <(rg -n '^\s*-?\s*uses:\s*' .github/workflows)

if rg -n 'releases/latest|ubuntu-latest|npx --yes' \
  .github/workflows Makefile .github/scripts compose.yaml; then
  echo "floating executable input found" >&2
  status=1
fi
exit "$status"
```

The policy explains full-SHA action pins with version comments, versioned download plus SHA-256, exact Cargo tools with `--locked`, npm lockfiles, image digests, and the explicit GitHub-hosted-runner/registry trust-root exception.

- [ ] **Step 2: Run the gate red on current CI**

Run: `bash .github/scripts/check-supply-chain.sh`

Expected: FAIL listing action major tags, `ubuntu-latest`, `releases/latest`, and unpinned `npx`.

- [ ] **Step 3: Pin workflow inputs**

Replace every runner with `ubuntu-24.04` and use:

```yaml
- uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
- uses: actions-rust-lang/setup-rust-toolchain@166cdcfd11aee3cb47222f9ddb555ce30ddb9659 # v1.17.0
- uses: EmbarkStudios/cargo-deny-action@3c6349835b2b7b196a839186cb8b78e02f7b5f25 # v2.1.1
- uses: crate-ci/typos@bee27e3a4fd1ea2111cf90ab89cd076c870fce14 # v1.48.0
- uses: actions/setup-java@03ad4de0992f5dab5e18fcb136590ce7c4a0ac95 # v5.6.0
  with:
    distribution: temurin
    java-version: "21.0.11+10"
```

Delete all `apt-get protobuf-compiler` steps; Task 3 makes them unnecessary.

- [ ] **Step 4: Pin TLA+ with verification on every run**

Add to `Makefile`:

<!-- markdownlint-disable MD010 -->

```make
TLA_TOOLS_VERSION := 1.7.4
TLA_TOOLS_SHA256 := 936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88
TLA_TOOLS_URL := https://github.com/tlaplus/tlaplus/releases/download/v$(TLA_TOOLS_VERSION)/tla2tools.jar

$(TLA_TOOLS):
	mkdir -p docs/formal/.tools
	curl -fsSL $(TLA_TOOLS_URL) -o $(TLA_TOOLS).tmp
	echo "$(TLA_TOOLS_SHA256)  $(TLA_TOOLS).tmp" | sha256sum -c
	mv $(TLA_TOOLS).tmp $(TLA_TOOLS)

tla: $(TLA_TOOLS)
	echo "$(TLA_TOOLS_SHA256)  $(TLA_TOOLS)" | sha256sum -c
	cd docs/formal && java -XX:+UseParallelGC -jar .tools/tla2tools.jar -config lease_protocol.cfg lease_protocol.tla
```

<!-- markdownlint-enable MD010 -->

- [ ] **Step 5: Lock Markdownlint**

Create:

```json
{
  "name": "koine-repository-tools",
  "private": true,
  "devDependencies": {
    "markdownlint-cli2": "0.22.1"
  }
}
```

Run `npm install --package-lock-only --ignore-scripts`, then change local/CI Markdownlint to `npm ci --ignore-scripts` followed by `npm exec -- markdownlint-cli2 ...`. Add `supply-chain` to `.PHONY` and `ci` dependencies.

- [ ] **Step 6: Run green and mutation-probe the gate**

```bash
make supply-chain
make md
make tla
```

Temporarily replace one checkout SHA with `@v7`; confirm `make supply-chain` fails, then revert that mutation with `apply_patch` before committing.

- [ ] **Step 7: Commit**

```bash
git add .apptlas/policies .github Makefile package.json package-lock.json
git commit -m "ci: pin executable supply-chain inputs"
```

### Task 3: Make protobuf generation hermetic

**Files:**

- Modify: `crates/koine-proto/Cargo.toml`
- Modify: `crates/koine-proto/build.rs`
- Modify: `Cargo.lock`
- Modify: `.github/workflows/ci.yml` if Task 2 left any protoc install.

**Interfaces:**

- Produces: build-time compiler path selected solely by `protoc-bin-vendored = "=3.2.0"`.

- [ ] **Step 1: Prove the current build trusts `PROTOC`**

Run with a fresh target directory:

```bash
task_target=$(mktemp -d)
PROTOC=/definitely/missing/protoc CARGO_TARGET_DIR="$task_target" cargo build -p koine-proto
```

Expected: FAIL with `failed to invoke protoc`.

- [ ] **Step 2: Add exact vendored compiler selection**

Add `protoc-bin-vendored = "=3.2.0"` under build dependencies and replace `build.rs` with:

```rust
//! Generates the koine.v1 gRPC contract with the pinned vendored compiler.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    let mut prost = tonic_prost_build::Config::new();
    prost.protoc_executable(protoc);
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_with_config(prost, &["proto/koine/v1/worker.proto"], &["proto"])?;
    Ok(())
}
```

This avoids unsafe environment mutation under edition 2024 and the workspace `unsafe_code = "forbid"` rule.

- [ ] **Step 3: Re-run with poisoned system compiler and validate dependencies**

```bash
task_target=$(mktemp -d)
PROTOC=/definitely/missing/protoc CARGO_TARGET_DIR="$task_target" cargo build -p koine-proto
cargo deny check
cargo machete
```

Expected: build passes using the vendored path; deny and machete are green. Update the existing machete justification only if generated-source detection changes.

- [ ] **Step 4: Commit**

```bash
git add crates/koine-proto Cargo.lock .github/workflows/ci.yml
git commit -m "build: vendor the protobuf compiler"
```

### Task 4: Harden image, manifests, and package boundaries

**Files:**

- Modify: `compose.yaml`
- Modify: `crates/koine-{store-postgres,grpc}/tests/support/mod.rs`
- Modify: root and all crate `Cargo.toml` files.

**Interfaces:**

- Produces: unchanged normalized dependency edges, explicit `publish = false`, immutable PG17 test/dev image.

- [ ] **Step 1: Capture dependency-edge evidence before edits**

```bash
cargo metadata --format-version 1 --no-deps \
  | jq '[.packages[] | {name, deps: [.dependencies[] | {name, path: .path, kind: .kind, rename: .rename}] | sort_by(.name, .kind)}] | sort_by(.name)' \
  > /tmp/koine-deps-before.json
```

- [ ] **Step 2: Pin Postgres by digest**

Use `postgres:17@sha256:a426e44bac0b759c95894d68e1a0ac03ecc20b619f498a91aae373bf06d8508d` in `compose.yaml`. In both test helpers import `testcontainers::ImageExt as _` and start:

```rust
Postgres::default()
    .with_tag("17@sha256:a426e44bac0b759c95894d68e1a0ac03ecc20b619f498a91aae373bf06d8508d")
    .start()
    .await
```

Run one Postgres test before continuing to prove the consumer accepts tag-plus-digest.

- [ ] **Step 3: Centralize every internal crate dependency**

Add root `[workspace.dependencies]` entries with `version = "0.1.0"` and paths such as `crates/koine-domain`; replace every internal dependency/dev-dependency declaration with `{ workspace = true }`. Do not centralize external crates in this task.

- [ ] **Step 4: Make publication intent and descriptions explicit**

Add `publish = false` to every crate package. Remove literal backticks from descriptions (`gRPC`, `OpenAPI`, `Postgres`, `OpenTelemetry`, `Prometheus` become plain text). Do not change package names or dependency features.

- [ ] **Step 5: Compare edges and scan manifests**

```bash
cargo metadata --format-version 1 --no-deps \
  | jq '[.packages[] | {name, deps: [.dependencies[] | {name, path: .path, kind: .kind, rename: .rename}] | sort_by(.name, .kind)}] | sort_by(.name)' \
  > /tmp/koine-deps-after.json
diff -u /tmp/koine-deps-before.json /tmp/koine-deps-after.json
rg -L '^publish = false$' crates/*/Cargo.toml
rg -n '^description = ".*`' crates/*/Cargo.toml
```

Expected: metadata diff and both ripgrep scans produce no output.

- [ ] **Step 6: Verify package file boundaries**

```bash
for crate in koine-domain koine-application koine-proto koine-store-memory koine-store-postgres koine-grpc koine-server; do
  cargo package --allow-dirty --list -p "$crate" > "/tmp/$crate-package-files.txt"
done
rg -n 'LICENSE|NOTICE|worker.proto|migrations' /tmp/koine-*-package-files.txt
```

Inspect each list: proto includes `worker.proto`; Postgres includes both migrations; all include license/notice as Cargo packages; source/build files are present.

- [ ] **Step 7: Run manifest gates and commit**

```bash
cargo build --workspace
cargo test --workspace
cargo deny check
cargo machete
git add Cargo.toml Cargo.lock crates/*/Cargo.toml compose.yaml crates/koine-store-postgres/tests/support crates/koine-grpc/tests/support
git commit -m "build: harden workspace package boundaries"
```

### Task 5: Reconcile public, architectural, and lifecycle truth

**Files:**

- Modify: `README.md`, `ROADMAP.md`, `CLAUDE.md`, `.env.example`.
- Modify: `.apptlas/epics/phase-2-data-plane.md`.
- Modify: relevant `docs/architecture/*.md`, `docs/formal/README.md`.
- Modify/move: legacy pinning and manifest items.

**Interfaces:**

- Consumes: all implemented behavior from the three hardening plans.
- Produces: honest phase state and closed historical acceptance criteria.

- [ ] **Step 1: Rewrite the public status without aspiration leakage**

README `Status` says pre-alpha, phase-2A closure hardening, and lists available today: event-sourced core, memory/Postgres adapters, transactional outbox, authenticated `koine.v1` gRPC worker surface, leases/heartbeat/recovery, and TLA+ model. Repair/resume, REST, MCP, CLI, dashboard, SDK, and conformance are explicitly labeled with their future phases. Building names pinned Rust plus no system protoc requirement; running documents Postgres and `KOINE_WORKER_TOKEN`.

- [ ] **Step 2: Reconcile roadmap, epic, and living context**

Until Task 6 passes, use exactly: `phase 2A implementation complete; zero-debt hardening active; phase 2B blocked`. Preserve every legitimate 2B item. Update item 1 formal properties, item 4 shared listener, and item 12 wiki evidence in the epic.

- [ ] **Step 3: Complete environment and architecture references**

`.env.example` contains all serve variables and comments from the resource plan. Wiki pages link ADR-0016/0017 and describe actual atomic retirement, one listener, pool budget, presence latency, vendored protoc, and CI pin policy without duplicating ADR rationale.

- [ ] **Step 4: Close the two legacy items truthfully**

Expand `ci-supply-chain-pinning.md` and `manifest-cleanup-workspace-deps.md` into template-complete records, mark every AC checked, record exact commands/diffs, add `Faithful`, and move both from `todo/` to `done/`. Do not close the operational item yet.

- [ ] **Step 5: Run docs gates and commit**

```bash
make typos
make md
git diff --check
git add README.md ROADMAP.md CLAUDE.md .env.example .apptlas docs/architecture docs/formal/README.md
git commit -m "docs: reconcile phase 2a operational truth"
```

### Task 6: Execute the zero-debt exit gate and unblock planning

**Files:**

- Modify/move: `.apptlas/backlog/ongoing/phase-2a-operational-closure.md`.
- Final state updates: `CLAUDE.md`, `ROADMAP.md`, `.apptlas/epics/phase-2-data-plane.md`.

**Interfaces:**

- Consumes: every previous task and plan.
- Produces: phase-2A closure; authorization to plan 2B, not to implement it.

- [ ] **Step 1: Run all automated gates fresh**

```bash
make supply-chain
make tla
make ci
cargo test -p koine-store-postgres
cargo test -p koine-grpc --test grpc_e2e
git diff --check
```

Record test/TLC counts and elapsed results, not merely exit code.

- [ ] **Step 2: Exercise the real product paths**

```bash
postgres_was_running=$(docker compose ps --status running -q postgres)
docker compose up -d postgres
cargo run -p koine-server -- dev-loop
cargo build -p koine-server
KOINE_WORKER_TOKEN=phase2a-smoke target/debug/koine-server serve > /tmp/koine-serve-smoke.log 2>&1 &
server_pid=$!
sleep 2
kill -INT "$server_pid"
wait "$server_pid"
rg -n 'authenticated grpc data plane' /tmp/koine-serve-smoke.log
if [ -z "$postgres_was_running" ]; then docker compose stop postgres; fi
```

Expected: dev-loop proves happy/retry/crash stories; server binds and exits zero on SIGINT. The real TCP/Postgres worker, heartbeat, expiry, and recovery evidence comes from the fresh `grpc_e2e` run in Step 1.

- [ ] **Step 3: Audit for closed-phase residue**

```bash
find .apptlas/backlog/todo -maxdepth 1 -type f ! -name .gitkeep -print
rg -n 'todo!\(|unimplemented!\(' crates
rg -n 'Phase 0.*in progress|phase 1 next|actions/.+@v[0-9]|releases/latest|ubuntu-latest' README.md ROADMAP.md CLAUDE.md .github Makefile
rg -L '^publish = false$' crates/*/Cargo.toml
git status --short
```

Expected before closing the operational item: only that item is in `ongoing`, no todo item, no fake-completeness macro, no stale/floating phrase, every crate non-publishable, clean tree.

- [ ] **Step 4: Obtain independent dual-verdict review**

Reviewer reads the accepted design and ADRs, checks all three plan diffs and task evidence, reruns high-risk concurrency/supply-chain/package commands, and returns spec-compliance plus quality verdicts. Critical/Important findings are fixed and re-reviewed; Minor findings are recorded and remain phase-blocking if attributable to 0/1/2A under the zero-debt policy.

If no independent agent was authorized, stop and request maintainer review rather than marking the phase complete.

- [ ] **Step 5: Close the item and update phase state only after review**

Fill evidence and `Faithful`; move the item to `done`. Then update living state to `Phase 2A complete and hardened — next: phase 2B planning (not started)`, retaining explicit future scope.

```bash
git mv .apptlas/backlog/ongoing/phase-2a-operational-closure.md .apptlas/backlog/done/
git add .apptlas/backlog/done/phase-2a-operational-closure.md CLAUDE.md ROADMAP.md .apptlas/epics/phase-2-data-plane.md
git commit -m "docs: close phase 2a zero-debt hardening"
```

- [ ] **Step 6: Re-run the final lightweight truth check**

```bash
find .apptlas/backlog/todo .apptlas/backlog/ongoing -maxdepth 1 -type f ! -name .gitkeep -print
git status --short --branch
git log --oneline --decorate -12
```

Expected: no active closed-phase item and a clean branch. Do not push without an explicit user request.

## 2026-07-21 applicable supply-chain audit amendment

The plan body above remains the exact pre-execution record from `1ddfa6f`.
Post-implementation audit supersedes only its executed Task 2 instructions;
the historical 0.22.1 selection is not rewritten.

The applicable repository-tool manifest is:

```json
{
  "name": "koine-repository-tools",
  "private": true,
  "packageManager": "npm@10.9.8",
  "engines": {
    "node": ">=22.23.1"
  },
  "devDependencies": {
    "js-yaml": "4.3.0",
    "markdownlint-cli2": "0.23.1"
  }
}
```

Both Markdownlint and supply-chain CI jobs run this setup before npm:

```yaml
- uses: actions/setup-node@a0853c24544627f65ddf259abe73b1d18a591444 # v5.0.0
  with:
    node-version: "22.23.1"
    package-manager-cache: false
- run: npm ci --ignore-scripts
```

Markdownlint still runs through `npm exec`. `make supply-chain` installs the
exact lock with scripts disabled, then invokes the fail-closed Bash wrapper,
semantic ESM checker, and repository-owned mutation suite. The checker parses
all workflow/Compose YAML and package/lock JSON with exact `js-yaml` 4.3.0,
rejects duplicate JSON keys, enumerates workflows independently of ignore
files, and enforces the reviewed action/comment, Node/npm, TLA+, gitleaks,
download, and image identities. These exact instructions preserve the
accepted immutable-input decision and resolve the applicable npm audit.

## 2026-07-21 applicable Operational Task 4 amendment

The temporary `postgres:17` exception described in the historical plan body
expired when Operational Task 4 started. The applicable implementation pins
Compose and both testcontainers consumers to
`postgres:17@sha256:a426e44bac0b759c95894d68e1a0ac03ecc20b619f498a91aae373bf06d8508d`.
The semantic checker contains no tag-only exception: it requires that exact
reviewed identity for the repository-owned Postgres service and its mutation
suite rejects both a tag-only reference and a syntactically valid wrong
digest.

Internal workspace dependency declarations retain their version, path, kind,
rename, feature, default-feature, target, optional, and registry metadata while
moving to root `[workspace.dependencies]` inheritance. Every crate declares
`publish = false`; Task 4 therefore records `cargo package --allow-dirty
--list` as file-boundary inspection, not as evidence that publishing is
enabled or that an archive was successfully built.

Every directory under `crates/` also carries regular-file `LICENSE` and
`NOTICE` copies byte-identical to the repository-root originals. The semantic
gate and its mutation suite fail when either file is absent, content-drifted,
or replaced by a symlink. This guard preserves the package-file requirement
without weakening the repository scanner's fail-closed symlink rule.
