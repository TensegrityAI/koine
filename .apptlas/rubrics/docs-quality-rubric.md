# Rubric: Documentation Quality

Applied in review to every `docs/architecture/` page touched (DoD item 4) and
to substantial README/policy changes. Score **pass / concern / fail** per
dimension; same severity mapping as
[code-review-rubric.md](code-review-rubric.md).

| Dimension | Pass looks like | Fail looks like |
| --- | --- | --- |
| **Complete template** | Page answers all four: what it does, how it is built, why (with ADR links), boundaries | A "what" with no "why"; ADRs restated instead of linked; boundaries unstated |
| **Truthfulness** | Describes what IS; planned content explicitly marked with its phase | Claims about unbuilt behavior; stale descriptions of replaced designs |
| **Navigability** | A newcomer can go from the page to the code (paths, crate names, key types named) and back | Prose about "the system" with nothing to grep for |
| **Freshness** | Matches the code as of this change — the page was updated in the same PR | The diff changes behavior the page describes, page untouched |
| **Signal density** | Every paragraph earns its place; diagrams where structure is non-obvious | Boilerplate sections, filler, duplicated rationale |

## Method

- Diff the page against the code change it accompanies: does the page still
  tell the truth after this diff?
- Spot-check one navigability claim: pick a type or path the page names and
  confirm it exists.
- For new pages: check the four template sections exist and the linked ADRs
  are the right ones.
