# Decide CI action pinning and typos version policy

- **State:** done
- **Origin:** phase 0 final review (2026-07-17), findings i+j
- **Epic:** [Phase 2 — Data plane](../../epics/phase-2-data-plane.md)

## Traceability

- **Implements:** [ADR 0017](../../../docs/adr/0017-hermetic-protobuf-ci-artifact-pinning.md), the [immutable executable-input policy](../../policies/supply-chain-policy.md), and [operational-closure plan Tasks 2 and 5](../../../docs/superpowers/plans/2026-07-21-koine-operational-closure.md).

## Acceptance criteria

- [x] AC1: the repository documents the decision between full-SHA and major-tag
  GitHub Action pins, including the exact `crate-ci/typos` identity, and
  `.github/workflows/ci.yml` implements it — *verify:* policy/ADR review and
  semantic workflow inspection by `make supply-chain`.
- [x] AC2: a fail-closed automated gate rejects floating or malformed
  executable inputs rather than trusting textual resemblance — *verify:*
  `make supply-chain` and its 73 repository-owned probes.
- [x] AC3: CI remains green with the selected pins and tool identities —
  *verify:* `make ci` after the final supply-chain and package-boundary changes.

## Dependencies

- [Close phase-2A operational and supply-chain debt](../ongoing/phase-2a-operational-closure.md)
  remains ongoing; this legacy decision can close without closing its parent
  or unblocking phase 2B.

## Historical opening record

This item opened in commit `a3fb420` as a six-line phase-0 finding. Its
original AC required a documented choice between action commit SHAs and major
tags, a choice between an exact `crate-ci/typos` version and floating `v1`, and
an updated `ci.yml`. Its original verification was “CI green after the
change.” The criteria above split that same requirement into independently
checkable statements; they do not replace or broaden its historical intent.

## Evidence

- ADR 0017 and the supply-chain policy select full 40-character action SHAs
  with reviewed release comments. CI uses exact `crate-ci/typos` v1.48.0 at
  `bee27e3a4fd1ea2111cf90ab89cd076c870fce14`; the semantic gate owns the
  allowlist and association checks.
- `git diff 1ddfa6f..fc4a651 -- .github/workflows/ci.yml Makefile package.json
  package-lock.json .github/scripts .apptlas/policies/supply-chain-policy.md`
  is the reviewed implementation range: immutable workflow/tool identities,
  the semantic checker, its mutation fixtures, exact Node/npm package graph,
  and policy.
- `make supply-chain` installs the exact lock with lifecycle scripts disabled,
  runs the checker, and passes all 73 current probes. The current checksums and
  the explicitly superseded historical probe counts/hashes live in the
  [Operational Task 2 report](../../../.superpowers/sdd/operational-task-2-report.md)
  rather than being duplicated here.
- Operational Task 4's final `make ci` passed after the last checker,
  immutable-image, manifest, and package-boundary changes. The exact command
  evidence and independent no-finding verdict are recorded in the
  [Operational Task 4 report](../../../.superpowers/sdd/operational-task-4-report.md).
- Independent Operational Task 2 review approved spec compliance and quality;
  the current 73-probe lexer closure is recorded by commits `fc4a651`,
  `883a27e`, and `36b7646`.

## Spec-fidelity statement

Faithful to ADR 0017, including its applicable audit amendment, and to the
original phase-0 findings i+j. Closing this legacy item does not close the
ongoing operational item or authorize phase 2B.
