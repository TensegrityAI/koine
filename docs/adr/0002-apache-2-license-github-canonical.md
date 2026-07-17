# 0002 — Apache-2.0 license, GitHub canonical

- **Status:** accepted
- **Date:** 2026-07-16

- **Context:** An open-source project needs an explicit license and a single canonical host settled at the outset, before contributors, CI, and community files (CODEOWNERS, SECURITY.md, CONTRIBUTING.md) can be built on top of it.

- **Decision:** License Koiné under Apache-2.0; host it canonically on GitHub.

- **Consequences:**
  - Easier: the explicit patent grant in Apache-2.0 matters for enterprise adoption confidence; a single canonical GitHub location gives one authoritative home for issues, PRs, and GitHub Actions-based CI.
  - Harder: Apache-2.0's NOTICE-file and patent-grant machinery is heavier than a bare-permissive license, and contributors expecting MIT's simplicity need a short onboarding note; the canonical-GitHub choice ties community tooling (CI, templates) to GitHub-specific mechanisms rather than staying platform-neutral.
  - Gave up: the option to also offer MIT's lower-friction adoption path alongside Apache-2.0.

- **Alternatives considered:**
  - Dual MIT/Apache-2.0 — rejected in favor of a single, clear license carrying the explicit patent grant.
  - GitLab canonical — rejected in favor of GitHub as the canonical host.
