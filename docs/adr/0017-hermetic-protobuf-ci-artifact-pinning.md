# 0017 — Hermetic protobuf compilation and CI artifact pinning

- **Status:** accepted
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

## 2026-07-21 applicable supply-chain audit amendment

Post-implementation audit found that the initially selected
`markdownlint-cli2` 0.22.1 lock resolved vulnerable `js-yaml` and `markdown-it`
versions, and that the first focused gate did not fail closed across every
policy category. The applicable reviewed identities are now
`markdownlint-cli2` 0.23.1, Node 22.23.1, npm 10.9.8, and
`actions/setup-node@a0853c24544627f65ddf259abe73b1d18a591444` with the
`v5.0.0` allowlist comment and disabled package-manager cache. Exact direct
`js-yaml` 4.3.0 supplies the semantic YAML/JSON policy parser.

This is an application amendment, not a status or architecture change. The
new exact identities remove the audited dependency findings and preserve the
accepted decision that executable inputs and their update surface remain
immutable and reviewable. The gate enumerates workflows from the filesystem,
parses policy-bearing YAML and JSON semantically, and fails closed on parser,
import, or filesystem errors.
