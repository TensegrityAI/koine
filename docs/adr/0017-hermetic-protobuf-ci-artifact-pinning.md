# 0017 — Hermetic protobuf compilation and CI artifact pinning

- **Status:** proposed
- **Date:** 2026-07-21
- **Context:** Phase-2A builds run `protoc` from a floating Ubuntu package,
  GitHub Actions use mutable major tags, TLA+ downloads a `latest` jar without
  an integrity check, and markdownlint resolves its executable and transitive
  dependencies at invocation time. These inputs can change without a repository
  diff, making a previously green build irreproducible and weakening review of
  executable supply-chain changes.
- **Decision:** Make repository-owned executable inputs immutable where their
  ecosystem permits it. Pin GitHub Actions to full commit SHAs with release
  comments; use versioned download URLs and checked-in SHA-256 values; run
  Cargo-installed tools at exact locked versions; install Node tooling from a
  committed lockfile; use version-specific hosted-runner labels and document
  unavoidable provider-managed mutability. Adopt an exact
  `protoc-bin-vendored` build dependency for `koine-proto` and fail explicitly
  on unsupported targets rather than falling back to an arbitrary system
  compiler. Enforce the policy with a focused CI check.
- **Consequences:** CI, local builds, and packaged builds use the same protobuf
  compiler selection and reviewed tool identities; supply-chain upgrades become
  explicit diffs. The repository gains vendored platform-specific protoc
  packages, a Node lockfile for markdownlint, checksum maintenance, and a pin
  update workflow. A GitHub-hosted runner and upstream registries remain trust
  roots and are documented as such; content pinning does not eliminate that
  trust.
- **Alternatives considered:** keep system `protoc` and document installation
  (not reproducible); pin an Ubuntu package version (runner-repository coupling
  and eventual package eviction); check generated protobuf Rust into source
  (drift from the first-class `.proto` contract); retain action major tags or
  `latest` downloads (unreviewed executable changes); build every tool from
  source in CI (larger attack and maintenance surface without a stronger
  identity guarantee).
