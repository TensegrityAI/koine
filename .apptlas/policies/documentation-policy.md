# Policy: Documentation

Docs are code: versioned in this repo, reviewed in the same PR as the change
they describe, linted by CI (typos, markdownlint). Documentation that lives
outside the PR flow cannot be required by the Definition of Done, so we don't
keep any there.

## The layers, and what belongs in each

| Layer | Location | Answers | Updated when |
| --- | --- | --- | --- |
| Architecture wiki | `docs/architecture/` | What each module does, how it is built, why (linking ADRs) | Same change that touches the module — DoD item 4 |
| Decisions | `docs/adr/` | Why we chose X over Y, with costs | When a boundary/guarantee/protocol changes ([../workflows/adr-workflow.md](../workflows/adr-workflow.md)) |
| API reference | rustdoc (`///`, `//!`) | How to use each public item | With the code; `missing_docs` is enforced |
| Living context | `CLAUDE.md` | Current phase, active plan | On phase transitions |
| Public face | `README.md` | What Koiné is, how to build it | When the outside story changes |

## Rules

1. **Describe what IS, not what is planned.** Aspirational content is allowed
   only when explicitly marked with its phase ("planned — phase 3"). A doc
   claiming unbuilt functionality as existing is fake completeness (DoD item 7).
2. **Every architecture page follows the template**: *What it does* (one
   paragraph), *How it is built* (structure, key types, data flow), *Why*
   (design forces, linked ADRs), *Boundaries* (what it depends on, what may
   depend on it). Diagrams (ASCII or mermaid) where structure is non-obvious.
3. **Pages link, never duplicate.** Rationale lives in ADRs; the wiki links
   ADR numbers instead of restating them. Duplication is how docs rot.
4. **English**, concise, present tense.
5. Quality is scored by
   [../rubrics/docs-quality-rubric.md](../rubrics/docs-quality-rubric.md) in
   review.

The wiki starts at [docs/architecture/README.md](../../docs/architecture/README.md);
per-crate pages are born with the phase that gives the crate real behavior
(DoD makes this automatic). Tooling upgrade to mdBook is planned for phase 3 —
the plain-markdown pages migrate as-is.
