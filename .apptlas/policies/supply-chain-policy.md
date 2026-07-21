# Policy: Immutable executable inputs

Repository-owned executable inputs are reviewable identities, not mutable
names. Supply-chain upgrades therefore land as explicit repository diffs and
pass `make supply-chain` before merge.

## Required pins

- GitHub Actions use full 40-character commit SHAs from the gate's reviewed
  action/version allowlist. The release comment is part of that allowlist;
  the gate does not claim to derive or cryptographically validate upstream
  tag-to-commit relationships. Repository-local actions (`./...`) are exempt
  from the SHA and release-comment form.
- Downloaded executables use a versioned URL and a checked-in SHA-256 digest.
  The digest is checked both when downloading and before every execution.
- The only approved Cargo installation is exactly
  `cargo install cargo-machete --version 0.9.2 --locked`; wrappers, omitted
  flags, Rustup `+toolchain` selectors, and other tools fail the gate.
- Node tools are exact direct dev dependencies in `package.json`: `js-yaml`
  `4.3.0` supplies the policy parser and `markdownlint-cli2` `0.23.1` supplies
  Markdownlint. They are installed from the committed `package-lock.json`
  with `npm ci --ignore-scripts`; Markdownlint runs with `npm exec`. CI uses
  Node `22.23.1` through the reviewed setup-node action, the repository
  declares npm `10.9.8`, and lifecycle scripts are forbidden throughout the
  lock graph.
- Container images use immutable digests when they are executable build, test,
  CI, or development inputs. The repository-owned Postgres service uses the
  reviewed Postgres 17 digest enforced by the semantic gate. Each of the two
  versioned Rust test helpers (`koine-store-postgres` and `koine-grpc`) must
  contain exactly one `Postgres::default().with_tag(...)` consumer with that
  same reviewed tag-plus-digest identity; a missing, duplicate, tag-only, or
  drifted consumer fails closed.
- Every workspace crate carries regular-file `LICENSE` and `NOTICE` copies that
  are byte-identical to the repository-root originals. Missing, drifted, or
  symlinked legal files fail the semantic gate before package inspection.

`make supply-chain` first performs the exact script-disabled lock install. A
small Bash wrapper then fails if Node or the installed parser is unavailable
and invokes the ESM policy checker. The checker enumerates every workflow from
the filesystem, independently of Git ignore rules, and semantically parses all
workflow and Compose YAML plus `package.json` and `package-lock.json`. Duplicate
JSON keys, malformed input, filesystem failures, parser import failures, and
unsupported action source forms all fail closed.

The checker enforces the exact action/comment allowlist, immutable runner and
image forms, setup-node and setup-java job associations and inputs, exact
Node/npm/package identities, exact TLA+ and gitleaks
download/checksum/execution sequences, and unique literal validated Makefile
targets. Any Make target definition containing `$(` or `${` in its left-hand
side fails closed instead of being evaluated.
It requires the exact eleven workspace crate directories under `crates/`:
`koine-application`, `koine-cli`, `koine-domain`, `koine-grpc`, `koine-http`,
`koine-mcp`, `koine-observability`, `koine-proto`, `koine-server`,
`koine-store-memory`, and `koine-store-postgres`. Every first-level entry must
be a real directory; symlinks and other filesystem objects are rejected
without being followed. Each directory requires a regular `Cargo.toml`,
`LICENSE`, and `NOTICE`, and both legal files are compared byte for byte with
the repository-root originals.
It enumerates repository-owned shell scripts from the filesystem while
excluding only declared generated, internal, and fixture trees. It rejects
the `-c`/`--command` option of `bash`, `sh`, `zsh`, or `dash`, including short
option clusters and preceding shell options, instead of attempting to
interpret nested code. Normal shell script invocation without a command option
remains allowed. The gate
rejects every non-allowlisted `curl`, `wget`, `npm`, `npx`, or `cargo install`
command across workflows, the Makefile, and those scripts. Its executable
mutation suite uses repository-owned fixtures and runs as part of
`make supply-chain`.

## Residual trust roots

Full content pins do not make execution fully hermetic. GitHub-hosted runner
images and upstream registries remain provider-managed trust roots. Versioned
runner labels narrow changes but do not identify an immutable machine image;
Cargo, npm, container, and release registries still control artifact
availability. These unavoidable roots are accepted, while identities that the
repository can pin remain mandatory.
