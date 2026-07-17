# Instructions: Documentation style

**Applies to:** `**/*.md`

- English, concise, present tense. Describe what IS; mark planned content
  with its phase ("planned — phase 3").
- Architecture pages follow the four-section template (what / how / why /
  boundaries) from
  [documentation-policy](../policies/documentation-policy.md).
- Link, don't duplicate: rationale lives in ADRs, referenced by number
  (`ADR 0006`), never restated.
- Reference code as `crate-name` / `path/to/file.rs` so readers can navigate
  from prose to code.
- Everything passes `typos` and `markdownlint` (config at repo root); run
  `make ci` before pushing docs-only changes too.
