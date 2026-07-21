# Operational Task 2 Report

## Status

Implemented the immutable executable-input policy and focused repository gate
from ADR-0017 and the Operational Task 2 brief. The backlog item remains
ongoing; this cut proves AC1 but does not close the multi-task operational
closure item.

Spec fidelity: faithful to ADR-0017 and Operational Task 2. No application
dependency or pin outside the brief changed.

## RED

`bash .github/scripts/check-supply-chain.sh` exited 1 before pinning. It
reported all 18 mutable GitHub Action references and separately listed:

- every `ubuntu-latest` runner;
- `releases/latest` in the TLA+ download;
- both `npx --yes` Markdownlint invocations.

The failure was caused by the intended floating inputs, not a script error.

## GREEN

- `make supply-chain` exited 0 after pinning.
- `make md` installed with `npm ci --ignore-scripts`, ran
  `markdownlint-cli2 0.22.1` through `npm exec`, and reported 0 errors across
  80 files.
- `make tla` verified the checked-in SHA-256 before execution and TLC reported
  no errors: 74,079 generated states, 18,598 distinct states, depth 24.
- A cold-download probe moved the cached jar aside, downloaded the versioned
  `v1.7.4` artifact, verified the temporary file, moved it atomically, verified
  the final file again, and completed TLC successfully.
- `bash -n .github/scripts/check-supply-chain.sh` exited 0. Shellcheck was not
  installed in the environment.
- The first post-`make md` `make typos` run exposed that the new untracked
  `node_modules` tree was not ignored and scanned third-party files. Adding
  `node_modules/` to `.gitignore` is the minimal fix; the repeated typos gate
  exits 0.
- The gate is mode `0755`. Neither `package.json` nor `package-lock.json`
  contains a `scripts` or `hasInstallScript` field.
- `make -n ci` includes `bash .github/scripts/check-supply-chain.sh`, and the
  GitHub workflow has a dedicated `make supply-chain` job.

## Mutation and parser probes

Replacing one pinned checkout with `actions/checkout@v7` made
`make supply-chain` exit 2 after reporting the exact floating Action. This
also proves the `status=1` assignment survives the `while` input mechanism;
the loop uses process substitution rather than a pipeline subshell. The
mutation was reverted with `apply_patch` and is absent from the final diff.

As a separate parser probe, a spaced local reference with an inline comment
(`./.github/actions/local-probe   # local action probe`) passed the gate. It was
also reverted. The committed workflow exercises external SHA pins with inline
release comments.

## Pins and checksums

- TLA+ `1.7.4` URL:
  `https://github.com/tlaplus/tlaplus/releases/download/v1.7.4/tla2tools.jar`
- Expected and observed TLA+ SHA-256:
  `936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88`
- `package-lock.json` SHA-256 at verification:
  `4da63e30588b7272f51b42ab5aa67ce8311d781ad6629d325e0d2cc9f13955fe`
- `check-supply-chain.sh` SHA-256 at verification:
  `a2133ef8e5e2ff1fc2317fe6a2fd1008c214681ef9f790447b2965e40df7a151`

## Files

- `.apptlas/policies/supply-chain-policy.md`
- `.apptlas/policies/README.md`
- `.gitignore`
- `.github/scripts/check-supply-chain.sh`
- `.github/workflows/ci.yml`
- `Makefile`
- `package.json`
- `package-lock.json`
- `.superpowers/sdd/operational-task-2-report.md`

## Concerns

- The protobuf `apt-get` steps are intentionally removed before Task 3 adds
  `protoc-bin-vendored`. Fresh hosted-runner clippy, test, and docs jobs can
  therefore fail to find `protoc` in this inter-task window. The local machine
  has `/usr/bin/protoc` 3.21.12, so a local full-CI run would not reproduce that
  hosted-runner gap; no fallback was added.
- `npm audit` reports three vulnerabilities in the required
  `markdownlint-cli2 0.22.1` graph: two moderate and one high, through
  `js-yaml` and `markdown-it`. The available remediation upgrades
  Markdownlint to 0.23.1, outside this brief's exact pin, so no dependency was
  changed.
- GitHub-hosted images and upstream registries remain provider-managed trust
  roots, as documented by the policy.
