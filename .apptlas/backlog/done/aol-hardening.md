# Harden the AOL: DoR/DoD gates, rubrics, workflows, instructions, wiki

- **State:** done
- **Origin:** maintainer request (2026-07-17): docs-as-DoD, spec-fidelity
  reflection, PM-principled AC, initial AOL asset pack before roadmap work
- **Epic:** none (pre-phase-1 governance)

## Traceability

- **Implements:** design spec §4 (governance layer), extending it with the
  DoR/AC/DoD gated lifecycle; maintainer decisions of 2026-07-17 (wiki
  in-repo at `docs/architecture/`, full asset pack approved)

## Acceptance criteria

- [x] AC1: DoD requires architecture-wiki updates and a spec-fidelity
  statement with divergence disposition — *verify:* read
  `policies/definition-of-done.md` items 4–5
- [x] AC2: a Definition of Ready gates entry to `ongoing/` with AC declared
  before work — *verify:* read `policies/definition-of-ready.md`;
  `workflows/task-lifecycle.md` shows both gates
- [x] AC3: three rubrics give objective scoring for code review, spec
  fidelity, and docs quality — *verify:* read `rubrics/`
- [x] AC4: instructions exist for rust-style, event-sourcing, testing,
  docs-style — *verify:* read `instructions/`
- [x] AC5: the wiki exists with an index and a truthful overview (phase-0
  status marked) — *verify:* read `docs/architecture/`
- [x] AC6: PR template carries the DoD checklist; backlog items have a
  template with AC/evidence/fidelity sections — *verify:* read
  `.github/pull_request_template.md`, `backlog/item-template.md`
- [x] AC7: all prose passes typos + markdownlint; CI green after merge —
  *verify:* `make ci` locally; Actions run on main

## Dependencies

- none

## Evidence (filled at close)

- `typos` + `npx markdownlint-cli2` + `make ci`: green (see commit gate output)
- Independent review: see review verdicts appended below
- CI on main: run following the merge commit (Actions)

## Spec-fidelity statement (filled at close)

Faithful, with one recorded extension: spec §4 described the governance
*structure*; this work adds the DoR gate, rubrics, and docs-as-DoD mechanics
on top. Extension direction was explicitly requested and approved by the
maintainer (2026-07-17); no spec statement is contradicted. `skills/` was
deliberately left empty (spec lists it; content deferred until real usage
patterns exist — noted in `.apptlas/README.md`).
