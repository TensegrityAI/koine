# Instructions: Rust style

**Applies to:** `crates/**/*.rs`

- Workspace lints are law: `unsafe_code = forbid`, clippy `all` + `pedantic`,
  `missing_docs` on public items. Never `#[allow]` your way past a lint
  without a comment stating the constraint that justifies it.
- No `unwrap()`/`expect()` outside `#[cfg(test)]`. Errors are typed per crate
  (`thiserror`), carry diagnostic context, and are propagated with `?` or
  handled deliberately — never swallowed.
- Public items get rustdoc that says what the item is *for*, not what the
  signature already says.
- Comments state constraints the code cannot express; never narration of the
  next line, never change-log prose.
- Follow the surrounding code's idiom. New patterns need a reason, and if the
  reason is architectural, an ADR.
