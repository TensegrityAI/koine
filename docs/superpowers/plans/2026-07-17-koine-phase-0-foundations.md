# Koiné Phase 0 — Foundations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Koiné workspace so that `cargo build` and the full CI pipeline are green from the first push: 11-crate workspace, `.apptlas/` governance layer, AGENTS.md, founding ADRs 0001–0009, hygiene tooling, git hooks, and GitHub Actions CI.

**Architecture:** Cargo workspace where hexagonal boundaries are crate boundaries (spec §2). Phase 0 creates all crates as documented stubs so the dependency graph — the architecture guardian — is locked in before any feature code exists. Governance and CI are code: every config is versioned, every check runs locally (lefthook) and remotely (Actions) identically.

**Tech Stack:** Rust 1.95.0 (edition 2024), cargo-deny, typos-cli, lefthook, GitHub Actions.

**Reference:** Spec at `docs/superpowers/specs/2026-07-16-koine-design.md`. Phase 0 success criterion: *`cargo build` + full CI green from first push.*

## Global Constraints

- Rust toolchain pinned to **exactly** `1.95.0` (never floating `stable`) — spec §4.
- `edition = "2024"`, `rust-version = "1.95"`, workspace `resolver = "3"`.
- Crate names exactly as spec §2: `koine-domain`, `koine-application`, `koine-proto`, `koine-store-postgres`, `koine-store-memory`, `koine-grpc`, `koine-http`, `koine-mcp`, `koine-observability`, `koine-server`, `koine-cli`.
- Dependency direction (compile-enforced): `domain` depends on nothing internal; `application` → `domain`; stores/adapters → `application` + `domain`; `server` → everything; `proto` standalone. Never the reverse.
- License Apache-2.0; every manifest inherits `license.workspace = true`.
- Clippy runs with `-D warnings` in CI and pre-push.
- lefthook must not invoke any binary that isn't installed by this plan (spec §4: no external binary deps like kineticrs' CLI).
- Commits follow Conventional Commits (`feat:`, `fix:`, `docs:`, `chore:`, `ci:`, `test:`, `refactor:`), enforced by `scripts/check-commit-message.sh`.
- All prose files must pass `typos`. Docs in English.

---

### Task 1: Toolchain pin and workspace skeleton

**Files:**
- Create: `rust-toolchain.toml`
- Create: `Cargo.toml` (workspace root)
- Create: `crates/<each of the 11 crates>/Cargo.toml` and `src/lib.rs` (or `src/main.rs` for `koine-server`, `koine-cli`)
- Modify: `.gitignore`

**Interfaces:**
- Consumes: nothing (first task).
- Produces: the workspace every later task builds inside; crate name list used verbatim by Tasks 6 (Makefile) and 7 (CI).

- [ ] **Step 1: Pin the toolchain**

```toml
# rust-toolchain.toml
[toolchain]
channel = "1.95.0"
components = ["rustfmt", "clippy", "rust-src", "rust-analyzer"]
profile = "minimal"
```

- [ ] **Step 2: Write the workspace root `Cargo.toml`**

```toml
[workspace]
resolver = "3"
members = [
    "crates/koine-domain",
    "crates/koine-application",
    "crates/koine-proto",
    "crates/koine-store-postgres",
    "crates/koine-store-memory",
    "crates/koine-grpc",
    "crates/koine-http",
    "crates/koine-mcp",
    "crates/koine-observability",
    "crates/koine-server",
    "crates/koine-cli",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.95"
license = "Apache-2.0"
repository = "https://github.com/kaelmans/koine"
authors = ["Marcos Saez <marcos.asp989@gmail.com>"]

[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"

[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
unwrap_used = "warn"
expect_used = "warn"
module_name_repetitions = "allow"
```

- [ ] **Step 3: Generate the 11 crate stubs**

Run this script from the repo root (creates identical documented stubs; binaries get `main.rs`):

```bash
set -euo pipefail
libs="koine-domain koine-application koine-proto koine-store-postgres koine-store-memory koine-grpc koine-http koine-mcp koine-observability"
bins="koine-server koine-cli"

desc() {
  case "$1" in
    koine-domain) echo "Koiné domain layer: aggregates, domain events, state machines. No I/O, no async, no infra deps." ;;
    koine-application) echo "Koiné application layer: use cases and driven ports (EventStore, OutboxRelay, ProjectionStore, LeaseManager, Clock, IdGenerator)." ;;
    koine-proto) echo "Koiné wire contract: versioned protobuf definitions and generated gRPC types." ;;
    koine-store-postgres) echo "Koiné Postgres driven adapter: event store, transactional outbox, projections." ;;
    koine-store-memory) echo "Koiné in-memory driven adapter for tests: complete port implementations without I/O." ;;
    koine-grpc) echo "Koiné data plane driving adapter: worker fetch stream, ack/fail, heartbeats, checkpoints over gRPC." ;;
    koine-http) echo "Koiné control plane driving adapter: REST API with OpenAPI, serves the embedded dashboard." ;;
    koine-mcp) echo "Koiné agent control plane driving adapter: MCP server over the same use cases." ;;
    koine-observability) echo "Koiné observability init: OpenTelemetry tracing, Prometheus metrics." ;;
    koine-server) echo "Koiné server binary: composition root wiring adapters to the application core." ;;
    koine-cli) echo "Koiné operator CLI: trace, queue and job operations against the control plane." ;;
  esac
}

for c in $libs $bins; do
  mkdir -p "crates/$c/src"
  cat > "crates/$c/Cargo.toml" <<EOF
[package]
name = "$c"
description = "$(desc "$c")"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true

[dependencies]

[lints]
workspace = true
EOF
done

for c in $libs; do
  printf '//! %s\n' "$(desc "$c")" > "crates/$c/src/lib.rs"
done

for c in $bins; do
  cat > "crates/$c/src/main.rs" <<EOF
//! $(desc "$c")

fn main() {
    println!("$c 0.1.0 — phase 0 stub");
}
EOF
done
```

- [ ] **Step 4: Declare the internal dependency edges (the architecture guardian)**

Append to the `[dependencies]` section of each listed manifest — these edges make illegal
imports uncompilable from day one:

```toml
# crates/koine-application/Cargo.toml
koine-domain = { path = "../koine-domain" }

# crates/koine-store-postgres/Cargo.toml  (same two lines for koine-store-memory,
# koine-grpc, koine-http, koine-mcp)
koine-domain = { path = "../koine-domain" }
koine-application = { path = "../koine-application" }

# crates/koine-server/Cargo.toml
koine-domain = { path = "../koine-domain" }
koine-application = { path = "../koine-application" }
koine-store-postgres = { path = "../koine-store-postgres" }
koine-store-memory = { path = "../koine-store-memory" }
koine-grpc = { path = "../koine-grpc" }
koine-http = { path = "../koine-http" }
koine-mcp = { path = "../koine-mcp" }
koine-observability = { path = "../koine-observability" }
```

(`koine-proto` and `koine-cli` gain their edges in phases 2–3; `koine-domain` never gains any.)

Stub crates with only doc comments don't reference their deps yet; silence the
unused-dep lint by adding one line under each dependent crate's doc comment in
`src/lib.rs` / `src/main.rs`, e.g. for `koine-application`:

```rust
use koine_domain as _;
```

(One `use <dep> as _;` line per declared internal dependency.)

- [ ] **Step 5: Extend `.gitignore`**

```gitignore
_archive/
target/
**/*.rs.bk
.env
```

- [ ] **Step 6: Verify the workspace builds clean**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo build --workspace && cargo test --workspace`
Expected: all four succeed; `cargo test` reports `0 passed; 0 failed` per crate.

- [ ] **Step 7: Commit**

```bash
git add rust-toolchain.toml Cargo.toml Cargo.lock crates/ .gitignore
git commit -m "chore: scaffold 11-crate workspace with compiled hexagonal boundaries"
```

---

### Task 2: Hygiene tooling configuration

**Files:**
- Create: `rustfmt.toml`, `deny.toml`, `typos.toml`, `.markdownlint.yaml`, `.editorconfig`

**Interfaces:**
- Consumes: workspace from Task 1.
- Produces: tool configs invoked verbatim by Task 6 (lefthook/Makefile) and Task 7 (CI): `cargo fmt --all --check`, `cargo deny check`, `typos`.

- [ ] **Step 1: Install the two missing dev tools (exact versions pinned)**

Run: `cargo install typos-cli --locked && cargo install lefthook --locked`
Expected: both binaries available (`typos --version`, `lefthook version`).

- [ ] **Step 2: Write `rustfmt.toml`**

```toml
edition = "2024"
max_width = 100
newline_style = "Unix"
use_field_init_shorthand = true
use_try_shorthand = true
```

- [ ] **Step 3: Write `deny.toml`**

```toml
[graph]
all-features = true

[advisories]
version = 2
yanked = "deny"

[licenses]
version = 2
allow = [
    "Apache-2.0",
    "MIT",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-3.0",
    "Zlib",
]

[bans]
multiple-versions = "warn"
wildcards = "deny"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
```

- [ ] **Step 4: Write `typos.toml`**

```toml
[files]
extend-exclude = ["_archive/", "target/", "Cargo.lock"]

[default.extend-words]
# Project vocabulary
koine = "koine"
```

- [ ] **Step 5: Write `.markdownlint.yaml`**

```yaml
default: true
MD013: false   # line length — tables and URLs make this noise
MD033: false   # inline HTML — needed for diagrams/badges
MD041: false   # first-line heading — ADR frontmatter breaks it
```

- [ ] **Step 6: Write `.editorconfig`**

```ini
root = true

[*]
charset = utf-8
end_of_line = lf
insert_final_newline = true
trim_trailing_whitespace = true
indent_style = space
indent_size = 4

[*.{md,yml,yaml,toml,json,js,ts,tsx}]
indent_size = 2
```

- [ ] **Step 7: Verify every tool passes on the current tree**

Run: `cargo fmt --all --check && cargo deny check && typos`
Expected: all three exit 0 (deny may print `advisories ok, licenses ok, bans ok, sources ok`).

- [ ] **Step 8: Commit**

```bash
git add rustfmt.toml deny.toml typos.toml .markdownlint.yaml .editorconfig
git commit -m "chore: add hygiene tooling (rustfmt, cargo-deny, typos, markdownlint, editorconfig)"
```

---

### Task 3: Legal and community files

**Files:**
- Create: `LICENSE`, `NOTICE`, `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CODEOWNERS`

**Interfaces:**
- Consumes: nothing beyond repo root.
- Produces: `README.md` sections that Task 4's AGENTS.md links to; LICENSE required by `cargo deny` license checks of our own crates.

- [ ] **Step 1: Fetch the canonical Apache-2.0 text**

Run: `curl -fsS https://www.apache.org/licenses/LICENSE-2.0.txt -o LICENSE && wc -l LICENSE`
Expected: `202 LICENSE` (canonical line count; any ~200 value is fine as long as the file starts with "Apache License").

- [ ] **Step 2: Write `NOTICE`**

```text
Koiné — event-sourced, language-agnostic job broker
Copyright 2026 Marcos Saez

Licensed under the Apache License, Version 2.0.
```

- [ ] **Step 3: Write `README.md`**

```markdown
# Koiné

> The common language between programming languages for background work.

**Koiné** (κοινή, *"the common [language]"*) is an event-sourced, language-agnostic
job broker written in Rust. The history of every job is the source of truth, not a
byproduct — which yields three capabilities no open-source broker combines:

- **Total traceability** — what happened, why, in what order, with what context;
  queryable and replayable, across languages.
- **Repair & resume** — a failed job is not "retried and hope": it is inspected,
  repaired, and continued from its last checkpoint, full history preserved.
- **Agent-native operation** — the control plane speaks MCP; agents are first-class
  operators, not an afterthought.

Workers connect over a versioned gRPC contract — SDKs are generated, not
reverse-engineered. Producers and operators use REST (OpenAPI), the CLI, or MCP.

## Status

**Pre-alpha.** Phase 0 (foundations) in progress. See
[`docs/superpowers/specs/2026-07-16-koine-design.md`](docs/superpowers/specs/2026-07-16-koine-design.md)
for the full design and build phases.

## Building

```bash
cargo build --workspace
cargo test --workspace
```

Requires the toolchain pinned in `rust-toolchain.toml` (rustup handles this
automatically).

## Architecture at a glance

Strict hexagonal architecture as an 11-crate workspace — the dependency graph *is*
the architecture guardian. Event log on Postgres as single source of truth;
synchronous dispatch projection for the hot path; transactional outbox for read
projections. See the design spec and `docs/adr/` for every decision and its
rationale.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
```

- [ ] **Step 4: Write `CONTRIBUTING.md`**

```markdown
# Contributing to Koiné

Thank you for considering a contribution!

## Ground rules

- **Read first:** `AGENTS.md` (operating contract), the design spec under
  `docs/superpowers/specs/`, and the ADRs under `docs/adr/`. Architectural
  decisions live there; PRs that contradict an accepted ADR need a superseding
  ADR, not a debate in the diff.
- **TDD:** tests accompany every behavior change. The three test rings (domain
  unit + proptest, application vs in-memory adapter, integration vs real
  Postgres) are described in the design spec §4.
- **Hexagonal boundaries are compile-enforced.** If your change needs a new
  dependency edge between crates, that is an architecture change: open an issue
  first.
- **Conventional Commits** (`feat:`, `fix:`, `docs:`, `chore:`, `ci:`, `test:`,
  `refactor:`) — enforced by the commit-msg hook.

## Local setup

```bash
rustup show                      # picks up rust-toolchain.toml
cargo install typos-cli lefthook --locked
lefthook install                 # git hooks: fmt/typos pre-commit, clippy/test pre-push
make ci                          # run everything CI runs
```

## Pull requests

- Keep PRs scoped to one concern.
- CI must be green: fmt, clippy (`-D warnings`), tests, cargo-deny, typos, gitleaks.
- New public items need doc comments (`missing_docs` is enforced).
```

- [ ] **Step 5: Write `SECURITY.md`**

```markdown
# Security Policy

## Supported versions

Pre-release: only the latest commit on `main` is supported.

## Reporting a vulnerability

Please report vulnerabilities privately via GitHub Security Advisories
("Report a vulnerability" on the repository's Security tab). Do **not** open a
public issue. You will receive an acknowledgement within 72 hours.

Scope notes for reporters: Koiné is a job broker — deserialization of untrusted
payloads, authentication of the data plane and control plane, and multi-tenant
queue isolation are the areas of highest interest.
```

- [ ] **Step 6: Write `CODEOWNERS`**

```text
* @kaelmans
```

- [ ] **Step 7: Verify prose passes checks**

Run: `typos && npx --yes markdownlint-cli2 "**/*.md" "!_archive" "!target"`
Expected: both exit 0.

- [ ] **Step 8: Commit**

```bash
git add LICENSE NOTICE README.md CONTRIBUTING.md SECURITY.md CODEOWNERS
git commit -m "docs: add legal and community files (Apache-2.0, README, contributing, security)"
```

---

### Task 4: Governance layer — AGENTS.md, CLAUDE.md, `.apptlas/`

**Files:**
- Create: `AGENTS.md`, `CLAUDE.md`
- Create: `.apptlas/README.md` and `README.md` in each of: `.apptlas/instructions/`, `.apptlas/backlog/` (+ `todo/`, `ongoing/`, `done/` with `.gitkeep`), `.apptlas/epics/`, `.apptlas/policies/`, `.apptlas/workflows/`, `.apptlas/incidents/`, `.apptlas/findings/`, `.apptlas/skills/`

**Interfaces:**
- Consumes: README.md (Task 3) and spec paths for links.
- Produces: the truth-hierarchy contract every future agent session reads first; backlog dirs used by all subsequent phases.

- [ ] **Step 1: Write `AGENTS.md`**

```markdown
# AGENTS.md — Koiné

> Operating contract for AI coding agents working in this repository.
> Scope: the whole workspace. Last updated: 2026-07-17.

## 0. Mission

Koiné is an event-sourced, language-agnostic job broker. The history of every job
is the source of truth. We build cathedral-grade foundations: small, verifiable,
reversible changes; no fake completeness; boundaries enforced by the compiler.

## 1. Read order

1. `AGENTS.md` — this contract
2. `CLAUDE.md` — living context and current phase
3. `docs/superpowers/specs/2026-07-16-koine-design.md` — the approved design
4. `docs/adr/INDEX.md` + ADRs relevant to the task
5. `.apptlas/backlog/` — active work items
6. The relevant code, manifests, migrations, proto files

## 2. Truth hierarchy

When sources conflict: **code and manifests → AGENTS.md → ADRs → design spec →
backlog → README/docs.** If code contradicts an accepted ADR, that is
architectural debt: report it, do not copy it as precedent.

## 3. Non-negotiables

- **TDD.** Failing test first, minimal implementation, green, commit.
- **Hexagonal boundaries are crate boundaries.** `koine-domain` has zero internal
  deps and no async/I/O. New inter-crate edges require an ADR.
- **Event log is append-only truth.** No mutation of recorded events, ever.
  State corrections are new events (`JobRepaired`, conflict events).
- **No fake completeness.** No `todo!()`, `unimplemented!()`, or docs claiming
  unwired functionality.
- **Conventional Commits**, enforced by hooks. CI green before merge.
- **Document non-obvious decisions** as ADRs (MADR format, `docs/adr/`).

## 4. Layout

- `crates/` — the 11-crate workspace (see design spec §2 for the crate map)
- `sdks/` — worker SDKs (phase 2+), `dashboard/` — embedded SPA (phase 3+)
- `.apptlas/` — agent operating layer: instructions, backlog (todo/ongoing/done),
  epics, policies, workflows, incidents, findings, skills
- `docs/adr/` — architecture decision records; `docs/superpowers/` — specs & plans

## 5. Commands

- `make ci` — everything CI runs (fmt, clippy -D warnings, test, deny, typos)
- `make test` / `make lint` / `make fmt` — individual rings
- `lefthook install` — git hooks (pre-commit: fmt+typos; pre-push: clippy+test;
  commit-msg: conventional commits)
```

- [ ] **Step 2: Write `CLAUDE.md`**

```markdown
# CLAUDE.md — Koiné living context

**Current phase: 0 — Foundations** (see design spec §6 for all phases).

Active plan: `docs/superpowers/plans/2026-07-17-koine-phase-0-foundations.md`.

## Quick orientation

- Start every session by reading `AGENTS.md`; it defines read order and the
  truth hierarchy.
- The design spec (`docs/superpowers/specs/2026-07-16-koine-design.md`) is
  approved — do not relitigate closed decisions (name, Postgres-first, gRPC
  data plane, event taxonomy, dispatch projection strategy).
- Phase 0 exit criterion: full CI green from first push.

## Phase log

- 2026-07-16 — Design spec approved and committed.
- 2026-07-17 — Phase 0 plan written; execution started.
```

- [ ] **Step 3: Create the `.apptlas/` tree**

Run:

```bash
set -euo pipefail
mkdir -p .apptlas/{instructions,epics,policies,workflows,incidents,findings,skills} \
         .apptlas/backlog/{todo,ongoing,done}
touch .apptlas/backlog/{todo,ongoing,done}/.gitkeep
```

Then write `.apptlas/README.md`:

```markdown
# Agent Operating Layer (`.apptlas/`)

Canonical home for Koiné's agent-operating assets, unifying what kineticrs kept
in `.agents/` + `.github/`. When the apptlas tool's canonical layout lands, this
directory adopts it; until then the kineticrs conventions apply.

- `instructions/` — scoped rules that apply by file pattern (rust style, event
  sourcing, tests, security, proto)
- `backlog/{todo,ongoing,done}` — work items; one file per item, moved between
  dirs as state changes
- `epics/` — multi-item initiatives mapping to design-spec phases
- `policies/` — standing rules (release, review, security)
- `workflows/` — repeatable operating procedures
- `incidents/` — post-mortems and conformance incident reports
- `findings/` — audit findings and architectural debt records
- `skills/` — repo-specific agent skills
```

And one-line `README.md` stubs in each subdirectory, e.g. for `instructions/`:

```markdown
# instructions/ — scoped rules applied by file pattern. Empty until phase 1 adds the first rules.
```

(Repeat the pattern for `epics/`, `policies/`, `workflows/`, `incidents/`, `findings/`, `skills/` with their one-line purpose from `.apptlas/README.md`.)

- [ ] **Step 4: Verify and commit**

Run: `typos && npx --yes markdownlint-cli2 "AGENTS.md" "CLAUDE.md" ".apptlas/**/*.md"`
Expected: exit 0.

```bash
git add AGENTS.md CLAUDE.md .apptlas/
git commit -m "docs: add agent operating layer (.apptlas), AGENTS.md contract, CLAUDE.md context"
```

---

### Task 5: Founding ADRs 0001–0009

**Files:**
- Create: `docs/adr/template.md`, `docs/adr/INDEX.md`, `docs/adr/0001-…` through `docs/adr/0009-…` (exact filenames in Step 2)

**Interfaces:**
- Consumes: decisions from design spec §1–§5 (copy rationale, don't invent).
- Produces: the ADR corpus AGENTS.md §3 refers to; INDEX.md format future ADRs follow.

- [ ] **Step 1: Write `docs/adr/template.md` (MADR-lite)**

```markdown
# NNNN — Title

- **Status:** proposed | accepted | superseded by NNNN
- **Date:** YYYY-MM-DD
- **Context:** What forces are at play? What problem does this decide?
- **Decision:** What we chose, stated imperatively.
- **Consequences:** What becomes easier, what becomes harder, what we gave up.
- **Alternatives considered:** Each rejected option and why.
```

- [ ] **Step 2: Write the nine ADRs**

Each follows the template, `Status: accepted`, `Date: 2026-07-16`. Content is
condensed from the design spec — the spec section named in each is the source;
copy its rationale faithfully. Files and required content:

1. `0001-koine-identity-and-name.md` — Context: NEXUS collided with Sonatype
   Nexus; "Rosetta" with Apple Rosetta/trademark. Decision: name the project
   Koiné; thesis "job history as source of truth" (spec §1). Consequences:
   crates.io names (`koine`, `koine-*`) must be reserved early. Alternatives:
   NEXUS, Rosetta, Telar, Relevo, Bitácora.
2. `0002-apache-2-license-github-canonical.md` — Decision: Apache-2.0 (patent
   grant for enterprise adoption), GitHub canonical host (spec §1). Alternatives:
   dual MIT/Apache-2.0, GitLab canonical.
3. `0003-multi-crate-workspace-compiled-hexagon.md` — Context: kineticrs audit
   showed single-crate lets layering erode (EventStore trait bound to a concrete
   aggregate). Decision: hexagonal boundaries as crate boundaries; dependency
   edges per plan Task 1 Step 4 (spec §2). Alternatives: single crate (kineticrs
   ADR-0003), module-level discipline.
4. `0004-event-log-single-source-of-truth.md` — Decision: all job/queue/worker
   state derives from an append-only event log; durable-execution event kinds
   (checkpoint, signal, approval) reserved in the v1 schema; heartbeats/progress
   ephemeral outside the log, threshold crossings are events (spec §1, §3).
   Alternatives: mutable state + audit log, hybrid.
5. `0005-postgres-event-store-behind-port.md` — Decision: `EventStore` is a port;
   first adapter Postgres (transactional, LISTEN/NOTIFY, battle-tested); complete
   in-memory adapter for tests guarantees port neutrality (spec §2). Alternatives:
   custom embedded log first (a project in itself), both from day one.
6. `0006-sync-dispatch-projection-async-outbox.md` — Decision: dispatch_queue
   updated in the same tx as the event append, fetched via SELECT … FOR UPDATE
   SKIP LOCKED; all other projections async via transactional outbox (event +
   outbox row, one tx) — closing kineticrs' dual-write gap (spec §3). Alternatives:
   all-async projections (dispatch lag), all-sync (throughput ceiling).
7. `0007-grpc-data-plane-rest-mcp-control-plane.md` — Decision: gRPC canonical
   data plane (typed contract, streaming, official codegen per language — the
   anti-Faktory move); REST+OpenAPI, MCP and CLI on the control plane (spec §2).
   Alternatives: WebSocket+JSON canonical, both first-class from v1.
8. `0008-at-least-once-leases.md` — Decision: at-least-once delivery via leases
   with TTL renewed by heartbeat; late acks recorded as conflict events; retries
   with exponential backoff + jitter; exhaustion → parked awaiting repair
   (spec §3). Alternatives: at-most-once, exactly-once claims (dishonest).
9. `0009-dashboard-vite-react-embedded.md` — Decision: Vite+React+TS SPA,
   OpenAPI-generated client, d3 for bespoke viz, static build embedded in
   koine-server via rust-embed — preserving single-binary deploys (spec §5).
   Alternatives: Leptos (d3 interop, iteration speed), copying todo-app Next.js
   frontend (Node runtime, GraphQL we don't expose).

- [ ] **Step 3: Write `docs/adr/INDEX.md`**

```markdown
# ADR Index

| # | Title | Status | Date |
|---|-------|--------|------|
| [0001](0001-koine-identity-and-name.md) | Koiné identity and name | accepted | 2026-07-16 |
| [0002](0002-apache-2-license-github-canonical.md) | Apache-2.0 license, GitHub canonical | accepted | 2026-07-16 |
| [0003](0003-multi-crate-workspace-compiled-hexagon.md) | Multi-crate workspace, compiled hexagon | accepted | 2026-07-16 |
| [0004](0004-event-log-single-source-of-truth.md) | Event log as single source of truth | accepted | 2026-07-16 |
| [0005](0005-postgres-event-store-behind-port.md) | Postgres event store behind a port | accepted | 2026-07-16 |
| [0006](0006-sync-dispatch-projection-async-outbox.md) | Sync dispatch projection, async outbox | accepted | 2026-07-16 |
| [0007](0007-grpc-data-plane-rest-mcp-control-plane.md) | gRPC data plane; REST+MCP control plane | accepted | 2026-07-16 |
| [0008](0008-at-least-once-leases.md) | At-least-once delivery with leases | accepted | 2026-07-16 |
| [0009](0009-dashboard-vite-react-embedded.md) | Dashboard: Vite+React SPA, embedded | accepted | 2026-07-16 |
```

- [ ] **Step 4: Verify and commit**

Run: `typos && npx --yes markdownlint-cli2 "docs/adr/*.md"`
Expected: exit 0.

```bash
git add docs/adr/
git commit -m "docs: add founding ADRs 0001-0009 with MADR template and index"
```

---

### Task 6: Git hooks, commit-message check, and Makefile

**Files:**
- Create: `scripts/check-commit-message.sh`, `lefthook.yml`, `Makefile`

**Interfaces:**
- Consumes: tool configs from Task 2; crate list from Task 1.
- Produces: `make ci` target invoked by CONTRIBUTING.md and CI (Task 7); commit-msg gate for all future commits.

- [ ] **Step 1: Write `scripts/check-commit-message.sh`**

```bash
#!/usr/bin/env bash
# Conventional Commits gate — no external dependencies (see AGENTS.md §3).
set -euo pipefail

msg_file="${1:?usage: check-commit-message.sh <commit-msg-file>}"
first_line="$(head -n1 "$msg_file")"

pattern='^(feat|fix|docs|chore|ci|test|refactor|perf|build)(\([a-z0-9._-]+\))?!?: .{1,72}$'

if [[ "$first_line" =~ $pattern ]]; then
    exit 0
fi

echo "✗ Commit message does not follow Conventional Commits:" >&2
echo "    $first_line" >&2
echo "  Expected: type(scope)?: subject   (type ∈ feat|fix|docs|chore|ci|test|refactor|perf|build)" >&2
exit 1
```

Run: `chmod +x scripts/check-commit-message.sh`

- [ ] **Step 2: Test the check script (both directions)**

Run: `echo "feat(domain): add job aggregate" > /tmp/cm-ok && scripts/check-commit-message.sh /tmp/cm-ok && echo PASS`
Expected: `PASS`

Run: `echo "added some stuff" > /tmp/cm-bad && scripts/check-commit-message.sh /tmp/cm-bad; echo "exit=$?"`
Expected: error message and `exit=1`.

- [ ] **Step 3: Write `lefthook.yml`**

```yaml
pre-commit:
  parallel: true
  commands:
    fmt:
      glob: "*.rs"
      run: cargo fmt --all --check
    typos:
      run: typos

commit-msg:
  commands:
    conventional:
      run: scripts/check-commit-message.sh {1}

pre-push:
  commands:
    clippy:
      run: cargo clippy --workspace --all-targets -- -D warnings
    test:
      run: cargo test --workspace
```

Run: `lefthook install`
Expected: `sync hooks: ✔` (or equivalent success output).

- [ ] **Step 4: Write `Makefile`**

```makefile
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
```

- [ ] **Step 5: Verify the full local pipeline**

Run: `make ci`
Expected: ends with `✓ all CI checks green`.

- [ ] **Step 6: Commit (this commit also proves the hooks fire)**

```bash
git add scripts/check-commit-message.sh lefthook.yml Makefile
git commit -m "chore: add git hooks, conventional-commit gate, and make ci pipeline"
```

Expected: pre-commit output shows fmt+typos running before the commit lands.

---

### Task 7: GitHub Actions CI

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `make ci` targets and tool configs from Tasks 2 & 6.
- Produces: the phase-0 exit criterion — CI green on first push.

- [ ] **Step 1: Write `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read

env:
  CARGO_TERM_COLOR: always

jobs:
  fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1  # reads rust-toolchain.toml
      - run: cargo fmt --all --check

  clippy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - run: cargo clippy --workspace --all-targets -- -D warnings

  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - run: cargo test --workspace

  docs:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - run: cargo doc --workspace --no-deps
        env:
          RUSTDOCFLAGS: -D warnings

  deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2

  typos:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: crate-ci/typos@v1

  gitleaks:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: gitleaks/gitleaks-action@v2
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

- [ ] **Step 2: Validate the workflow syntax locally**

Run: `npx --yes yaml-lint .github/workflows/ci.yml || python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml')); print('yaml ok')"`
Expected: `yaml ok` (or lint success).

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add GitHub Actions pipeline (fmt, clippy, test, docs, deny, typos, gitleaks)"
```

---

### Task 8: Phase 0 closeout — publish and verify exit criterion

**Files:**
- Modify: `CLAUDE.md` (phase log)
- Create: `.apptlas/backlog/done/phase-0-foundations.md`

**Interfaces:**
- Consumes: everything above.
- Produces: green CI on GitHub = phase 0 exit criterion met; phase log entry the phase 1 plan will reference.

- [ ] **Step 1: Create the GitHub repository and push** ⚠️ *outward-facing — confirm with Marcos before running; requires `gh auth login` or manual repo creation at github.com/new*

```bash
gh repo create kaelmans/koine --public --source . --push \
  --description "Koiné — event-sourced, language-agnostic job broker. The common language between languages for background work."
```

(If `gh` is unavailable: create the repo in the GitHub UI, then
`git remote add origin git@github.com:kaelmans/koine.git && git push -u origin main`.)

- [ ] **Step 2: Verify CI is green on the first push**

Run: `gh run watch --exit-status` (or check the Actions tab).
Expected: all 7 jobs pass. **This is the phase 0 exit criterion.** If any job
fails, fix forward (the local `make ci` mirror should have caught everything
except action-environment differences).

- [ ] **Step 3: Reserve the crates.io names** ⚠️ *outward-facing — confirm with Marcos; requires `cargo login`. Publishing 0.1.0 stubs reserves the names (crates.io has no other reservation mechanism); `koine` verified free 2026-07-16*

```bash
for c in koine-domain koine-application koine-proto koine-store-postgres \
         koine-store-memory koine-grpc koine-http koine-mcp \
         koine-observability koine-server koine-cli; do
  cargo publish -p "$c" --allow-dirty || break
done
```

Note: also claim the bare `koine` name with a placeholder crate if desired
(`cargo new koine --lib` in a temp dir pointing its description at the repo).
Alternatively, skip this step entirely and accept squatting risk until phase 2.

- [ ] **Step 4: Record closeout**

Append to `CLAUDE.md` phase log:

```markdown
- 2026-07-XX — Phase 0 complete: CI green on first push. Phase 1 (event-sourced core) next.
```

Write `.apptlas/backlog/done/phase-0-foundations.md`:

```markdown
# Phase 0 — Foundations

- **State:** done
- **Exit criterion:** full CI green from first push — met (see Actions run on initial push)
- **Plan:** docs/superpowers/plans/2026-07-17-koine-phase-0-foundations.md
- **Delivered:** 11-crate workspace, hygiene tooling, legal/community files,
  AGENTS.md + CLAUDE.md + .apptlas/, ADRs 0001–0009, hooks + Makefile, CI.
```

- [ ] **Step 5: Final commit**

```bash
git add CLAUDE.md .apptlas/backlog/done/phase-0-foundations.md
git commit -m "docs: close out phase 0 — foundations delivered, CI green"
git push
```

---

## Not in this plan (deliberately)

- **Postgres/docker-compose** — arrives with phase 1 (first real store adapter).
- **`koine-proto` contents, build.rs, tonic deps** — phase 2.
- **`dashboard/` scaffolding and its CI job** — phase 3 (spec §6: after REST read endpoints exist).
- **`.apptlas/instructions/*` scoped rules** — written in phase 1 alongside the first real code they govern (rules before code = rules invented in a vacuum).
- **Coverage tooling, release workflow, docs site** — later phases when there is something to measure/release/document.
