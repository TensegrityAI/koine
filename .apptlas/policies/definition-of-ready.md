# Policy: Definition of Ready

An item may move from `todo/` to `ongoing/` only when all of the following
hold. Work started on a not-ready item is the root cause of scope drift,
untestable "done" claims, and spec divergence discovered months late — the
entry gate exists to make those failures impossible, not to add ceremony.

## Ready checklist

1. **Acceptance criteria written** — each one an observable, verifiable
   statement ("a worker crash releases the lease within TTL", not "handle
   crashes"). Given/When/Then form is welcome where it clarifies. Each AC
   names its **verification method**: which test ring proves it
   ([testing-policy.md](testing-policy.md)), or the manual/e2e exercise where
   rings don't reach.
2. **Traceability links present**: the design-spec section(s) and/or ADR(s)
   this item implements, and the plan task it belongs to (if any). An item
   that implements nothing in the spec is either a spec change (write the
   ADR first) or scope creep (reject it).
3. **Dependencies and blockers listed** — other items, external decisions,
   missing infrastructure. An item blocked on an unmade decision is not ready.
4. **Scoped to one review cycle** — if a reviewer could not meaningfully
   approve or reject it as a unit, split it before starting.
5. **Uses the item template** ([../backlog/item-template.md](../backlog/item-template.md)),
   so evidence and the spec-fidelity statement have a place to land at close.

## Who checks

Whoever moves the file to `ongoing/` asserts readiness and is accountable for
it. A reviewer finding an unready item mid-flight sends it back to `todo/`
with the missing points named — that is a normal outcome, not a conflict.
