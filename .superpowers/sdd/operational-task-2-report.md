# Operational Task 2 Report

## Current status

Operational Task 2 has a fail-closed semantic supply-chain gate in commits
`348625d1731c29556d6432be5edfa449bd59d8a0` and
`0c7f28e1a9e5756f61d9440816031c0335b565c0`, with final command-parser closure
in `ba66ff4f887c5a4404e3a027faa44fae06a6ff3a`, immutable Postgres closure in
`9888c7c`, crate legal-file integrity in `1d8e1ed`, and Rust-helper lexer
closure in `fc4a651`. The Bash entrypoint
requires the
reviewed Node runtime and installed parser, while the ESM checker enumerates
workflows and repository-owned shell scripts directly from the filesystem and
parses policy-bearing YAML and JSON before enforcing exact executable
identities. Its repository-owned suite currently has 73 passing probes.

The parent operational-closure item remains ongoing. This cut supplies current
evidence for AC1 but does not mark it complete, close the item, or claim the
independent review required by the Definition of Done.

Spec fidelity: faithful to ADR-0017 and the dated applicable audit amendments.
No application dependency or executable identity outside Operational Task 2
changed.

## Current verification

- `make supply-chain` exits 0 after `npm ci --ignore-scripts`; all 73 mutation
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
  `cbc6abe0076d8029ed62e531cb97d0c783861cc6b995ec1eb22cead9b9c5eb14`
- Mutation suite SHA-256:
  `145e4148816850fa91a9481b686774e346d032c89dd2517ef0aeff74b4e4989a`
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

I1 GREEN at that review point: all 51 then-current probes passed. Malformed
YAML/JSON, duplicate JSON keys, missing files, ignored workflows, scanner
symlinks, missing Node/parser, and parser import failures produced non-zero
results.

Command-parser RED: before commit `ba66ff4`, the checker accepted
`bash --noprofile -c`, `cargo +stable install`, and a Make target whose
left-hand side was `$(TLA_ALIAS)`. GREEN adds those three class regressions,
retains explicit `dash -lc` and `rustup run stable cargo install` coverage, and
proves that a normal `bash --noprofile ./script.sh` invocation remains allowed.
Operational Task 4 adds wrong-digest image and drifted, missing, and symlinked
crate legal-file regressions. All 61 probes at that review point passed.

Rust-helper lexer RED: the source regex accepted an unpinned executable
`Postgres::default().start()` when the approved full chain appeared in a nested
block comment or raw string. GREEN in `fc4a651` lexes identifiers,
punctuation, numbers, and normal/raw strings, ignores line and nested block
comments, and rejects unsupported or unterminated lexical forms. All 73 current
probes pass; comments and unrelated strings cannot satisfy the executable
consumer contract.

## Enforced policy surface

- Every workflow under `.github/workflows`, including ignored and nested files,
  is discovered through filesystem APIs and parsed as exactly one YAML
  document. Compose YAML and both package files are parsed semantically.
- External actions must match the exact SHA and adjacent release-comment
  allowlist. Unsupported inline, quoted-key, or flow action forms fail closed;
  repository-local block actions remain allowed.
- All jobs use `ubuntu-24.04`. Every workflow and Compose image requires an
  immutable digest. The canonical Postgres service must use the exact reviewed
  Postgres 17 identity; tag-only and wrong-digest mutations fail. Both Rust
  helpers must each contain exactly one executable token chain equivalent to
  `Postgres::default().with_tag(EXACT_STRING).start(`, and every executable
  `Postgres::default()` must be that chain.
- Setup-node is allowed only in the canonical Markdownlint and supply-chain
  jobs, with exact Node, cache, step-order, and npm-install associations.
- Setup-java is allowed only in the canonical TLA job, with exact Temurin
  distribution, Java `21.0.11+10`, and step ordering.
- TLA+ version, URL, digest, download recipe, pre-execution checksum, and Java
  invocation are exact. Gitleaks version 8.24.3, URL, digest, extraction, and
  execution are exact.
- The only allowed Cargo installation is exact cargo-machete 0.9.2 with
  `--locked`; Rustup selectors and wrappers fail. Validated Makefile targets
  are literal and unique; dynamic left-hand-side expansion fails closed.
- All repository-owned `.sh`, `.bash`, and `.zsh` scripts are enumerated from
  the filesystem outside the declared internal/generated/fixture exclusions.
  The command options `-c` and `--command` for `bash`, `sh`, `zsh`, and `dash`
  are rejected, including short clusters and preceding options, without
  interpreting nested code. Normal script invocation remains allowed.
- All other executable `curl`, `wget`, `npm`, and `npx` command forms are
  rejected across workflows, the Makefile, and enumerated shell scripts,
  including environment, `command`, substitution, and chain wrappers.
- Package and lock semantics enforce exact direct pins, registry URLs,
  SHA-512 integrity, Node/npm contracts, and the absence of lifecycle scripts.
- The exact eleven workspace crate directories each have regular-file
  `LICENSE` and `NOTICE` copies byte-identical to the repository root; missing,
  extra, non-directory, symlinked, drifted, or absent entries fail closed.

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
amendments and reports that described 23, 44, 51, 57, 58, 61, or 71 probes,
and their associated checker/test hashes, are likewise superseded by the
current 73-probe suite and current hashes above. In particular, these identities
are historical only and are not current verification:

- 61-probe checker:
  `1557d9637667d48cd98073c4edb826061ed434a30d19f5b949e2c142f899bc7a`;
  suite: `fe606107683d9fdaf5d75b8caf66471b380e074541b7ab3c1c5f80ad8f9aaca5`.
- 71-probe checker:
  `75d82046f1d617721b45539bd5a51a963cfe867e3b0731d5c1e6cbb4d5c4e931`;
  suite: `45c2b91a6b1a74cf31b0a7babe5fd3a428bfe9f64dfe35ce5465443408e50d16`.

## Remaining concerns and owners

- Operational Task 3 removed the hosted-runner `protoc` gap with the exact
  vendored compiler; no apt install or system-compiler fallback remains.
- Operational Task 4 removed the temporary Postgres exception and its gate
  branch in `9888c7c`; no image-policy exception remains. Commit `1d8e1ed`
  prevents the package legal-file copies from drifting.
- GitHub-hosted runner images and upstream registries remain provider-managed
  trust roots, as recorded in the policy.
