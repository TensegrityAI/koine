# Contributing to Koiné

Thank you for considering a contribution!

## Ground rules

- **Read first:** `AGENTS.md` (operating contract), the design spec under
  `docs/superpowers/specs/`, and the ADRs under `docs/adr/`. Architectural
  decisions live there; PRs that contradict an accepted ADR need a superseding
  ADR, not a debate in the diff.
- **TDD:** tests accompany every behavior change. The three test rings (domain
  unit + proptest, application vs in-memory adapter, integration vs real
  Postgres) are described in the design spec §4.
- **Hexagonal boundaries are compile-enforced.** If your change needs a new
  dependency edge between crates, that is an architecture change: open an issue
  first.
- **Conventional Commits** (`feat:`, `fix:`, `docs:`, `chore:`, `ci:`, `test:`,
  `refactor:`) — enforced by the commit-msg hook.

## Local setup

```bash
rustup show                      # picks up rust-toolchain.toml
cargo install typos-cli lefthook --locked
lefthook install                 # git hooks: fmt/typos pre-commit, clippy/test pre-push
make ci                          # everything CI runs except gitleaks (CI-only)
```

## Pull requests

- Keep PRs scoped to one concern.
- CI must be green: fmt, clippy (`-D warnings`), tests, docs build, cargo-deny, typos, gitleaks.
- New public items need doc comments (`missing_docs` is enforced).
