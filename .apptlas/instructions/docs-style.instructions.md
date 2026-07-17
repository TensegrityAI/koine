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
- Everything passes `typos` and `markdownlint` (config at repo root; `make md`
  runs the exact CI check); run `make ci` before pushing docs-only changes too.
- **Scope exemption:** `docs/superpowers/` (specs, plans) holds immutable
  execution artifacts — the same append-only discipline as ADRs and the event
  log. They are excluded from markdownlint and never edited for cosmetics.
