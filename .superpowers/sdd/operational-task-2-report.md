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

## 2026-07-21 review disposition

The maintainer made `markdownlint-cli2` 0.23.1, Node 22.23.1, npm 10.9.8, and
`actions/setup-node@a0853c24544627f65ddf259abe73b1d18a591444` (`v5.0.0`)
the applicable exact identities. ADR-0017, the hardening design, and the
operational plan carry dated application amendments; accepted status is
unchanged.

- I1 is resolved by a fail-closed allowlist gate plus executable fixture
  suite. Twenty-three probes cover tags, missing/wrong action comments,
  quoted and flow `uses`, both npx flags, comment suppression, scanner
  failures, TLA download/run checksums, npm/package/setup-node/Node drift,
  image policy, and unapproved curl/wget downloads.
- I2 is resolved by the exact setup-node/Node/npm identities and the regenerated
  0.23.1 lock. `npm audit --json` reports zero vulnerabilities; `make md`
  reports `markdownlint-cli2` 0.23.1 and zero issues.
- The Minor image finding is explicit debt rather than an implicit bypass:
  only `compose.yaml`'s `postgres:17` is temporarily allowed. **Owner:**
  Operational Task 4. **Deadline:** before phase-2A operational closure. Any
  drift or additional non-digest image fails the mutation suite.
- The previously reported 0.22.1 audit concern is superseded by this
  disposition. The inter-task `protoc` gap remains owned by Task 3; no apt or
  fallback was reintroduced.
- Follow-up review verified that `markdownlint-cli2` 0.23.1 declares Node
  `>=22`, without an upper bound. The repository tool contract is therefore
  `>=22.23.1`, while CI remains reproducibly pinned to Node 22.23.1. A specific
  mutation rejects reintroducing `<23`; the consolidated quoted/flow fixture
  keeps the suite at 23 probes. On the Node 24.14.0 host, `make md` now runs
  without `EBADENGINE` and reports zero issues.

Updated verification checksums:

- `package-lock.json`:
  `b8aa9d3690f3ecefc465843e7981efc81228b4aeadd18cd67b83570a8aa82b15`
- `.github/scripts/check-supply-chain.sh`:
  `b936d8645e27695f2822c62d2207b109b8c208ddfcbd6f974c8250f3c8cc7c63`
- `.github/scripts/test-supply-chain.sh`:
  `4c9b93c52826f1066a8676c3bdd62d9d5906198350d285cd0de63f37628114be`
