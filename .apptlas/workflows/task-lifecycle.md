# Workflow: Task Lifecycle

Every unit of work is a file in `.apptlas/backlog/`, created from
[../backlog/item-template.md](../backlog/item-template.md), moving through
three directories with a gate at each transition.

```text
            [Definition of Ready]              [Definition of Done]
  todo/  ──────────────────────▶  ongoing/  ──────────────────────▶  done/
    ▲        gate: DoR met            │          gate: DoD met, both
    │                                 │          review verdicts clean
    └────────── sent back ◀───────────┘
         (DoR gap found mid-flight,
          or DoD point failed)
```

## Transitions

1. **Created → `todo/`**: anyone (human or agent) may file an item. Minimum
   at creation: title, origin, and enough description to judge it later.
   Unready items live here indefinitely — that is what `todo/` is for.
2. **`todo/` → `ongoing/`**: whoever moves it asserts the
   [Definition of Ready](../policies/definition-of-ready.md) and becomes
   accountable for that assertion. AC and traceability links are filled in
   *before* the move.
3. **`ongoing/` → `done/`**: only after every
   [Definition of Done](../policies/definition-of-done.md) point holds, with
   evidence and the spec-fidelity statement recorded *in the item file*, and
   a non-implementer review with both verdicts clean
   ([review-policy](../policies/review-policy.md)).
4. **Backwards moves are normal**: a failed DoD point or a mid-flight DoR gap
   sends the item back with a note naming what was missing. No stigma; the
   gates working is the system working.

## Bookkeeping rules

- The item file is the record: AC evidence, review verdicts, fix rounds, and
  divergence dispositions are appended to it, not scattered in chat logs.
- Findings from reviews (Minor and up) that aren't fixed in-flight become new
  `todo/` items citing the review as origin — never a silently dropped list.
- Epics (`../epics/`) group items per design-spec phase; an item belonging to
  an epic links it.
