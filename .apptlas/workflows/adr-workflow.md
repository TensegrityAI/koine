# Workflow: Architecture Decision Records

ADRs live in `docs/adr/`, numbered sequentially, MADR-lite format
([docs/adr/template.md](../../docs/adr/template.md)), indexed in
[docs/adr/INDEX.md](../../docs/adr/INDEX.md).

## When an ADR is required

Any change that alters:

- an **inter-crate dependency edge** (the compiled hexagon — AGENTS.md
  non-negotiable);
- a **guarantee**: delivery semantics, event-log append-only-ness, lease
  behavior, transactional boundaries;
- the **wire contract's shape or versioning strategy** (`koine-proto`);
- a **significant external dependency or tool adoption** (new storage
  backend, new CI infrastructure, new runtime);
- a **previously accepted ADR's decision** — see superseding, below.

When in doubt, the test is: *would a future maintainer ask "why is it like
this?"* If yes, record it.

## Process

1. Draft from the template with the next free number. Status `proposed`.
2. Context states the forces honestly; Consequences include costs, not only
   benefits; Alternatives name what was actually considered and why rejected.
3. Review like any change (it rides the PR that implements the decision, or
   its own PR for decision-only records). Maintainer acceptance flips status
   to `accepted`; add the INDEX row in the same commit.
4. **Superseding, never rewriting**: an accepted ADR's decision text is
   immutable — the same append-only discipline as our event log. To change
   course, write a new ADR that names what it supersedes; mark the old one
   `superseded by NNNN`. Clarifying notes may be appended, clearly dated,
   below the original text.

## Spec reconvergence

When an ADR amends something the design spec states, the same change updates
the spec (or marks the spec section as superseded by the ADR) — code, spec,
and ADRs must never disagree silently
([../rubrics/spec-fidelity-rubric.md](../rubrics/spec-fidelity-rubric.md)).
