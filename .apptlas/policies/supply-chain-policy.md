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
- Cargo-installed tools use an exact version with `--locked`.
- Node tools are exact dev dependencies in `package.json`, installed from the
  committed `package-lock.json` with `npm ci --ignore-scripts`, and run with
  `npm exec`. CI uses Node `22.23.1` through the reviewed setup-node action,
  the repository declares npm `10.9.8`, and lifecycle scripts are forbidden.
- Container images use immutable digests when they are executable build or CI
  inputs. Development-only images without a stable reviewed digest must be
  called out explicitly.

The fail-closed gate scans `.github/workflows`, `Makefile`, `compose.yaml`, and
repository-owned `.github/scripts`. It rejects scanner/read failures,
non-allowlisted actions or version comments, mutable runners, floating npm
execution, unapproved executable downloads, missing download/run checksums,
Node/npm/package drift, and unapproved image tags. Its executable mutation
suite uses repository-owned fixtures and runs as part of `make supply-chain`.

## Temporary image exception

`compose.yaml` may contain exactly `postgres:17` without a digest until
**Operational Task 4** replaces it with the approved PostgreSQL digest. This
is a narrow, gate-enforced exception: any other tag or image without a digest
fails. The owner is Operational Task 4 and the deadline is before phase-2A
operational closure; the exception and its gate branch are removed together.

## Residual trust roots

Full content pins do not make execution fully hermetic. GitHub-hosted runner
images and upstream registries remain provider-managed trust roots. Versioned
runner labels narrow changes but do not identify an immutable machine image;
Cargo, npm, container, and release registries still control artifact
availability. These unavoidable roots are accepted, while identities that the
repository can pin remain mandatory.
