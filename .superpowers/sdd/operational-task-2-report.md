# Operational Task 2 Report

## Current status

Operational Task 2 now has a fail-closed semantic supply-chain gate in commit
`348625d1731c29556d6432be5edfa449bd59d8a0`. The Bash entrypoint requires the
reviewed Node runtime and installed parser, while the ESM checker enumerates
workflows directly from the filesystem and parses policy-bearing YAML and JSON
before enforcing exact executable identities. Its repository-owned suite has
44 passing probes, including the reported inline, ignored-file, structural,
parser, lockfile, command-wrapper, image, runner, and setup-node bypass cases.

The parent operational-closure item remains ongoing. This cut supplies current
evidence for AC1 but does not mark it complete, close the item, or claim the
independent review required by the Definition of Done.

Spec fidelity: faithful to ADR-0017 and the dated applicable audit amendments.
No application dependency or executable identity outside Operational Task 2
changed.

## Current verification

- `make supply-chain` exits 0 after `npm ci --ignore-scripts`; all 44 mutation
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
  `6cabe356b95bbcb172e5680c3b3e52ba18595e535dd1e0cd316c38d00920edbb`
- Mutation suite SHA-256:
  `4700d1fa85ddff536884354a477084efe5749723761b39dc2e69deac187fe88d`
- Direct `js-yaml` integrity:
  `sha512-1td788aAnnZ5qs7V2QIRl1owjtYpbKt749Y3xauqQgwIIGF/xXWz1wMTEBx5O3LK3lXLVuqXPdPxj2BoFHaW9Q==`

## Review RED and GREEN

RED: the pre-review textual gate accepted the `second_inline_uses` mutation,
which placed a second `uses` entry on a line whose first entry was approved.
That demonstrated that line scanning could miss valid YAML structure even when
the allowlist itself was exact.

GREEN: the replacement checker semantically loads every workflow and compares
all discovered actions with a deliberately narrow, unquoted block-source form.
The original bypass and the other 43 probes now pass. Malformed YAML/JSON,
duplicate JSON keys, missing files, ignored workflows, missing Node/parser,
and parser import failures all produce non-zero results.

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
- TLA+ version, URL, digest, download recipe, pre-execution checksum, and Java
  invocation are exact. Gitleaks version 8.24.3, URL, digest, extraction, and
  execution are exact.
- All other executable `curl`, `wget`, `npm`, and `npx` command forms are
  rejected across workflows, the Makefile, and repository-owned shell scripts,
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
amendments that described 23 probes are likewise superseded by the current
44-probe suite. They are not presented as current verification.

## Remaining concerns and owners

- Task 3 still owns the deliberate hosted-runner `protoc` gap. No apt install
  or system-compiler fallback was reintroduced in this cut.
- Task 4 still owns the exact `compose.yaml` `postgres:17` exception. Its
  deadline remains before phase-2A operational closure; the exception and gate
  branch must be removed together.
- GitHub-hosted runner images and upstream registries remain provider-managed
  trust roots, as recorded in the policy.
