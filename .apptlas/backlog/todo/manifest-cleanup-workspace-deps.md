# Centralize internal deps and clean manifest descriptions

- **State:** todo
- **Origin:** phase 0 final review (2026-07-17), findings a+b
- **AC:** internal crate dependencies declared once in root `[workspace.dependencies]` (path + version) with consumers using `workspace = true`; `[package]` description fields free of literal backticks.
- **Verification:** `cargo build`, `cargo test`, `cargo deny check` green; `cargo metadata` shows identical dependency edges before/after.
- **When:** before first crates.io publish (phase 2).
