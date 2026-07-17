# Rubric: Code Review

Score each dimension **pass / concern / fail**. Any *fail* is at least an
Important finding; *concerns* are Minor findings and must be recorded (see
[../policies/review-policy.md](../policies/review-policy.md)). The rubric
exists so two different reviewers — human or agent — reach the same verdict
on the same diff.

| Dimension | Pass looks like | Fail looks like |
| --- | --- | --- |
| **Correctness** | Behavior matches the AC under normal and failure paths; concurrency and error interleavings considered | A reachable input/state produces wrong output, lost data, or a panic |
| **Boundary fidelity** | Changes respect the hexagonal edges (domain ← application ← adapters); new inter-crate edges have an ADR | Domain importing infra, adapter logic in use cases, an edge added silently |
| **Guarantee honesty** | Delivery/event-log guarantees (append-only, at-least-once, lease semantics) hold after the change | An event mutated in place, an ack path that can drop information, a "we'll assume it succeeded" |
| **Test quality** | Tests assert observable behavior, include the failure path, would break if the behavior regressed | Tests that assert nothing, mirror the implementation line-by-line, or cover only the happy path |
| **Error handling** | Errors are typed, propagated or handled deliberately, and carry enough context to diagnose | Swallowed errors, `unwrap()` outside tests, stringly-typed errors where a type exists |
| **Simplicity (YAGNI)** | The diff does what the item needs and no more | Speculative abstractions, config for imagined futures, dead code shipped "for later" |
| **Idiom & clarity** | Reads like the surrounding code; names say what things are; comments state constraints, not narration | Style islands, misleading names, commented-out code |

## Method

- Read the item's AC first; review against *them*, not against taste.
- Reproduce at least one load-bearing claim per review (run the covering
  test, exercise the flow) — proportional to risk.
- Findings cite file:line and a concrete fix; verdicts without evidence
  don't count.
