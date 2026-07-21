# Policy: Immutable executable inputs

Repository-owned executable inputs are reviewable identities, not mutable
names. Supply-chain upgrades therefore land as explicit repository diffs and
pass `make supply-chain` before merge.

## Required pins

- GitHub Actions use full 40-character commit SHAs with a release-version
  comment. Repository-local actions (`./...`) are exempt from the SHA form.
- Downloaded executables use a versioned URL and a checked-in SHA-256 digest.
  The digest is checked both when downloading and before every execution.
- Cargo-installed tools use an exact version with `--locked`.
- Node tools are exact dev dependencies in `package.json`, installed from the
  committed `package-lock.json` with `npm ci --ignore-scripts`, and run with
  `npm exec`.
- Container images use immutable digests when they are executable build or CI
  inputs. Development-only images without a stable reviewed digest must be
  called out explicitly.

The focused gate scans `.github/workflows`, `Makefile`, `compose.yaml`, and
repository-owned `.github/scripts`. It rejects non-local GitHub Actions that
are not pinned to a full commit SHA, mutable release download paths, floating
Ubuntu runner labels, and runtime-resolved `npx --yes` tools.

## Residual trust roots

Full content pins do not make execution fully hermetic. GitHub-hosted runner
images and upstream registries remain provider-managed trust roots. Versioned
runner labels narrow changes but do not identify an immutable machine image;
Cargo, npm, container, and release registries still control artifact
availability. These unavoidable roots are accepted, while identities that the
repository can pin remain mandatory.
