# Policy: Definition of Done

Applies to every backlog item, plan task, and feature. A task is **not done**
because its tests pass — it is done when its *product* has been evaluated
against what was expected, and the repository (code, tests, docs) tells the
truth about what now exists.

Done is the exit gate of the task lifecycle. The entry gate is
[definition-of-ready.md](definition-of-ready.md); the acceptance criteria the
DoD verifies are declared there, before work starts — never reconstructed
from memory afterwards.

## Definition of Done (all points, checked before closing)

1. **All acceptance criteria verified by their declared method** — evidence
   (test names, commands run, output) recorded in the item file. The question
   each AC answers: *does this meet what was expected when the item was
   written?*
2. **The deliverable was exercised end-to-end as a product**, not only through
   unit tests: run the binary/flow the change affects and observe the behavior.
3. **TDD followed**: tests written first, seen failing, then green. No test
   added after the fact "to cover" already-written code without justification.
4. **Architecture wiki updated** (`docs/architecture/`): every module touched
   has its page updated in the same change; a new module gets a new page
   (what it does, how it is built, why — linking the ADRs that shaped it).
   See [documentation-policy.md](documentation-policy.md). A module without a
   current page is not done.
5. **Spec-fidelity statement**: the item file names the spec sections / ADRs
   this work implements, and states either "faithful" or lists each divergence
   with its disposition (finding filed, or ADR amended). Writing the wiki page
   is the reflection mechanism — you cannot document honestly what silently
   drifted. **A divergence absorbed silently is a DoD failure**, scored by
   [../rubrics/spec-fidelity-rubric.md](../rubrics/spec-fidelity-rubric.md).
6. **Reference docs current**: rustdoc on public items; ADR added/superseded if
   a boundary or guarantee changed; CLAUDE.md phase log if phase state changed.
7. **CI green; hooks passed; no fake completeness** (`todo!()`, stubs described
   as working, unwired features documented as features).
8. **Reviewed by someone other than the implementer** (subagent reviewer
   counts), with both verdicts required by
   [review-policy.md](review-policy.md): spec compliance and quality.

## Failure handling

An item that fails any point goes back to `ongoing` with a note — never close
"with known gaps" silently. Gaps become explicit follow-up items in `todo/`,
each meeting the Definition of Ready before anyone picks them up.
