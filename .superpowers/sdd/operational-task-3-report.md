# Operational Task 3 Report

## Current review status — 2026-07-22

Independent slice review is complete. Exact verdict: **Spec: Faithful.
Quality: Approved — 0 findings.** Durable evidence is recorded in the parent
operational item's
[Slice review evidence](../../.apptlas/backlog/ongoing/phase-2a-operational-closure.md#slice-review-evidence-2026-07-22).
The parent remains `ongoing`, and AC2 remains unchecked until Task 6.

The pending-review statements in the implementation snapshot and self-review
below are historical pre-review evidence and are superseded by this dated
verdict.

## Implementation snapshot before review (historical)

`koine-proto` now selects the executable from exact build dependency
`protoc-bin-vendored = "=3.2.0"` and passes it directly to
`tonic_prost_build::Config::protoc_executable`. It does not mutate the process
environment or fall back to a system compiler. Operational Task 2 had already
removed every CI installation and invocation of `protoc`; this task does not
reintroduce one.

The parent operational-closure item remains ongoing. This cut supplies current
evidence for AC2 but leaves its checkbox open until independent review and the
parent item's remaining acceptance criteria and Definition of Done gates pass.

Spec fidelity: faithful to ADR-0017, hardening design §6, and Operational Task
3. No application dependency edge or protobuf wire contract changed.

## TDD proof

### RED: the previous build trusted `PROTOC`

The pre-change build used a fresh target outside the repository:

```text
RED_TARGET=/tmp/koine-task3-red.aZYc4u
PROTOC=/definitely/missing/protoc \
  CARGO_TARGET_DIR=/tmp/koine-task3-red.aZYc4u \
  cargo build -p koine-proto
RED_EXIT=101
```

Its exact cause was:

```text
Error: Custom { kind: NotFound, error: "Could not find `protoc`. If `protoc` is installed, try setting the `PROTOC` environment variable to the path of the `protoc` binary. To install it on Debian, run `apt-get install protobuf-compiler`. It is also available at https://github.com/protocolbuffers/protobuf/releases  For more information: https://docs.rs/prost-build/#sourcing-protoc" }
```

This is the current `prost-build` wording for the brief's expected missing
compiler failure. Exit 101 and the diagnostic prove that the old build trusted
the poisoned environment.

### GREEN: the vendored path ignores poisoned host selection

After the minimal implementation, the same poison passed from another fresh
target:

```text
GREEN_TARGET=/tmp/koine-task3-green.A5Ep34
PROTOC=/definitely/missing/protoc \
  CARGO_TARGET_DIR=/tmp/koine-task3-green.A5Ep34 \
  cargo build -p koine-proto
Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.92s
GREEN_EXIT=0
```

The green run compiled `protoc-bin-vendored` 3.2.0 and its 3.2.0 platform
packages before generating and compiling `koine-proto`. Both temporary targets
remain under `/tmp`, outside the worktree, because that mount does not support
recoverable trash. They create no repository artifacts.

## Dependency and quality evidence

`cargo tree -p koine-proto --edges build` reports exactly these direct build
dependencies:

```text
koine-proto v0.1.0
[build-dependencies]
├── protoc-bin-vendored v3.2.0
└── tonic-prost-build v0.14.6
```

`cargo tree -i protoc-bin-vendored` reports the expected reverse chain:
`protoc-bin-vendored` → `koine-proto` → `koine-grpc` → `koine-server`.

- `cargo deny check` exits 0: advisories, bans, licenses, and sources are OK.
  Its configured duplicate-version warnings remain non-failing and are
  unrelated to this dependency.
- `cargo machete` exits 0 and finds no unused dependencies. The existing
  generated-source justification needs no change.
- `cargo fmt --all -- --check` exits 0.
- `PROTOC=/definitely/missing/protoc cargo check --workspace --all-targets`
  exits 0.
- `PROTOC=/definitely/missing/protoc cargo clippy --workspace --all-targets --
  -D warnings` exits 0.
- `PROTOC=/definitely/missing/protoc cargo test --workspace` exits 0 on its
  final run: all 127 tests pass, including the real-Postgres and real-gRPC
  integration suites, and every doctest passes.
- `.github/workflows/ci.yml` contains no `protoc`, `protobuf-compiler`, or
  protobuf apt-install step, so the workflow remains unchanged.

One preceding workspace-suite attempt encountered a transient Testcontainers
connection response (`unexpected response from SSLRequest: 0x48`) in
`happy_path_records_the_full_story`. The exact test passed immediately in
isolation, and the complete 127-test workspace run then passed without a code
or environment change. No unrelated store change is included in this cut.

## Files changed

- `crates/koine-proto/Cargo.toml`: exact vendored build dependency.
- `crates/koine-proto/build.rs`: direct vendored compiler selection through
  `Config::protoc_executable`; no unsafe environment mutation.
- `Cargo.lock`: exact vendored crate and platform packages at 3.2.0.
- `docs/architecture/koine-proto.md`: current build mechanism and ADR-0017
  boundary.
- `.apptlas/backlog/ongoing/phase-2a-operational-closure.md`: Task 3 evidence,
  with AC2 deliberately left open.
- `.superpowers/sdd/operational-task-3-report.md`: reproducible RED/GREEN and
  quality evidence.

## Self-review

- The manifest uses the required exact `=3.2.0` requirement.
- `build.rs` matches the approved `compile_with_config` design and obtains its
  executable solely from `protoc_bin_vendored::protoc_bin_path()`.
- No `std::env::set_var`, unsafe block, host fallback, wire-contract change, CI
  package install, or cargo-machete exception was added.
- The diff is limited to the implementation, lock graph, architecture truth,
  lifecycle evidence, and this report.
- Independent spec-compliance and quality verdicts remain pending. **Historical
  pre-review statement; superseded by the 2026-07-22 verdict above.**
