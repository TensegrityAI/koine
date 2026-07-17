# Rubric: Spec Fidelity

Answers the spec-compliance verdict of every review and DoD item 5. The
question is never "is this good code?" — it is "*is this what we said we
would build?*", with divergence made visible instead of absorbed.

## The five questions

1. **Coverage**: does the work implement everything the cited spec sections /
   ADRs / AC require? List anything missing.
2. **Scope**: does it contain anything the spec did *not* ask for? Additions
   are findings — sometimes justified, never silent.
3. **Semantics**: are the guarantees exactly as specified — or quietly
   weakened ("at-least-once" that can drop a late ack, "append-only" with an
   update path, "same transaction" that became eventually-consistent)?
   Weakened semantics are Critical.
4. **Naming & shape**: do the types, events, states, and protocol elements
   match the spec's vocabulary? Silent renames break traceability between
   spec, code, and wiki.
5. **Disposition**: is every divergence found above already recorded — as a
   finding, an ADR amendment, or an explicit maintainer decision? Divergence
   with disposition can be healthy evolution. Divergence without disposition
   is the failure mode this rubric exists to catch.

## Verdicts

| Verdict | Meaning |
| --- | --- |
| **Faithful** | Questions 1–4 clean |
| **Divergent — documented** | Divergences exist and every one has a disposition (5). Acceptable; the record is the point |
| **Divergent — silent** | Any divergence without disposition. Spec compliance ❌; blocks closure |

When the divergence is *right* and the spec is wrong: amend the spec via ADR
(see [../workflows/adr-workflow.md](../workflows/adr-workflow.md)) — the code
and the spec must reconverge in the same change, whichever direction.
