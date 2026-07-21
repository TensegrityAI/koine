# Operational Task 2 Report

## Current status

Operational Task 2 has a fail-closed semantic supply-chain gate in commits
`348625d1731c29556d6432be5edfa449bd59d8a0` and
`0c7f28e1a9e5756f61d9440816031c0335b565c0`. The Bash entrypoint requires the
reviewed Node runtime and installed parser, while the ESM checker enumerates
workflows and repository-owned shell scripts directly from the filesystem and
parses policy-bearing YAML and JSON before enforcing exact executable
identities. Its repository-owned suite currently has 51 passing probes.

The parent operational-closure item remains ongoing. This cut supplies current
evidence for AC1 but does not mark it complete, close the item, or claim the
independent review required by the Definition of Done.

Spec fidelity: faithful to ADR-0017 and the dated applicable audit amendments.
No application dependency or executable identity outside Operational Task 2
changed.

## Current verification

- `make supply-chain` exits 0 after `npm ci --ignore-scripts`; all 51 mutation
  and fail-closed probes pass.
- `npm audit --json` reports 0 vulnerabilities at every severity.
- `make md` installs the exact lock, runs `markdownlint-cli2` 0.23.1, and
  reports 0 issues across 80 files.
- `make typos` exits 0.
- `make tla` verifies SHA-256 before execution and completes with no error:
  74,079 generated states, 18,598 distinct states, and depth 24.
- `node --check .github/scripts/check-supply-chain.mjs`, Bash syntax checks for
  the wrapper, test runner, and shell fixtures, and `git diff --check` exit 0.
- `package.json` has exact direct `js-yaml` 4.3.0 and
  `markdownlint-cli2` 0.23.1 dev dependencies, npm 10.9.8, and Node
  `>=22.23.1`. The lock contains exact registry and integrity identities and no
  lifecycle scripts.

Current verification identities:

- `package-lock.json` SHA-256:
  `55b4835322064ecb736a88cae3ae7867fce0dac4e9bb55b54b230a815bcb5906`
- Bash wrapper SHA-256:
  `70aec3c4af9b6b4796b1fa51443066e6c1306b7ab44057af126ae59628a42e42`
- ESM checker SHA-256:
  `7091b1de1e80d731b3e81cf28c50feacfbadf804935371459dbaa81e946bac71`
- Mutation suite SHA-256:
  `7670add0c4098ab22a2208e0ddb40630f17437b5cec65b3ef012b43a52a6c9ff`
- Direct `js-yaml` integrity:
  `sha512-1td788aAnnZ5qs7V2QIRl1owjtYpbKt749Y3xauqQgwIIGF/xXWz1wMTEBx5O3LK3lXLVuqXPdPxj2BoFHaW9Q==`

## Review RED and GREEN

Initial review RED: the pre-semantic textual gate accepted an inline-action
bypass, establishing that line scanning could miss valid YAML structure. The
fixture now contains two real flow-sequence actions on one line—the first exact
and the second floating—and fails with the exact unapproved-action diagnostic.

Final I1 RED: before commit `0c7f28e`, the checker accepted six repository
fixtures covering incomplete and wrapped Cargo installs, setup-java `latest`,
`sh -c` download indirection, a download in `scripts/check-commit-message.sh`,
and a duplicate `tla` target. It also accepted a shell-scanner symlink failure.

GREEN: all 51 current probes pass. Malformed YAML/JSON, duplicate JSON keys,
missing files, ignored workflows, scanner symlinks, missing Node/parser, and
parser import failures all produce non-zero results.

## Enforced policy surface

- Every workflow under `.github/workflows`, including ignored and nested files,
  is discovered through filesystem APIs and parsed as exactly one YAML
  document. Compose YAML and both package files are parsed semantically.
- External actions must match the exact SHA and adjacent release-comment
  allowlist. Unsupported inline, quoted-key, or flow action forms fail closed;
  repository-local block actions remain allowed.
- All jobs use `ubuntu-24.04`. Every workflow and Compose image requires an
  immutable digest except the one exact, temporary `postgres:17` exception.
- Setup-node is allowed only in the canonical Markdownlint and supply-chain
  jobs, with exact Node, cache, step-order, and npm-install associations.
- Setup-java is allowed only in the canonical TLA job, with exact Temurin
  distribution, Java `21.0.11+10`, and step ordering.
- TLA+ version, URL, digest, download recipe, pre-execution checksum, and Java
  invocation are exact. Gitleaks version 8.24.3, URL, digest, extraction, and
  execution are exact.
- The only allowed Cargo installation is exact cargo-machete 0.9.2 with
  `--locked`. Validated Makefile targets are unique.
- All repository-owned `.sh`, `.bash`, and `.zsh` scripts are enumerated from
  the filesystem outside the declared internal/generated/fixture exclusions.
  Shell `-c` indirection is rejected without interpreting nested code.
- All other executable `curl`, `wget`, `npm`, and `npx` command forms are
  rejected across workflows, the Makefile, and enumerated shell scripts,
  including environment, `command`, substitution, and chain wrappers.
- Package and lock semantics enforce exact direct pins, registry URLs,
  SHA-512 integrity, Node/npm contracts, and the absence of lifecycle scripts.

## Authority and history preservation

The operational plan body is byte-for-byte identical to its `1ddfa6f` form;
the applicable corrections exist only in its dated appendix. ADR-0017 and the
accepted hardening design retain their original bodies and only append the
dated application amendment. Accepted status and architecture are unchanged.

## Historical and superseded evidence

Commit `bced29b` was the initial Operational Task 2 implementation. Its textual
gate, 0.22.1 Markdownlint graph, reported probe count, hashes, and vulnerability
finding are historical evidence only and are superseded by the current status,
identities, semantic checker, audit result, and checksums above. Later review
amendments and reports that described 23 or 44 probes, and their associated
checker/test hashes, are likewise superseded by the current 51-probe suite and
current hashes above. They are not presented as current verification.

## Remaining concerns and owners

- Task 3 still owns the deliberate hosted-runner `protoc` gap. No apt install
  or system-compiler fallback was reintroduced in this cut.
- Task 4 still owns the exact `compose.yaml` `postgres:17` exception. Its
  deadline remains before phase-2A operational closure; the exception and gate
  branch must be removed together.
- GitHub-hosted runner images and upstream registries remain provider-managed
  trust roots, as recorded in the policy.
