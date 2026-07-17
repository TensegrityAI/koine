# Policy: Definition of Done & Acceptance Criteria

Applies to every backlog item, plan task, and feature. A task is **not done**
because its tests pass — it is done when its *product* has been evaluated.

## Acceptance Criteria (declared at creation)

Every backlog item and plan task declares, before work starts:

- **AC:** observable behaviors that must hold, phrased as verifiable statements
  ("a worker crash releases the lease within TTL", not "handle crashes").
- **Verification method:** which test ring(s) prove each AC (domain unit /
  proptest, application vs in-memory, integration vs Postgres, conformance
  suite), plus manual/e2e exercise where rings don't reach.

## Definition of Done (checked before closing)

1. All AC verified by the declared method — evidence (test names, commands run,
   output) recorded in the task file.
2. **The deliverable was exercised end-to-end as a product**, not only through
   unit tests: run the binary/flow the change affects and observe the behavior.
3. TDD followed: tests written first, seen failing, then green. No test added
   after the fact "to cover" already-written code without justification.
4. Docs current: rustdoc on public items, ADR if a boundary or guarantee
   changed, CLAUDE.md phase log if phase state changed.
5. CI green; hooks passed; no fake completeness (`todo!()`, stubs documented as
   working, unwired features).
6. Reviewed against the plan/spec by someone other than the implementer
   (subagent reviewer counts).

An item that fails any point goes back to `ongoing` with a note — never close
"with known gaps" silently; gaps become explicit follow-up items in `todo/`.
