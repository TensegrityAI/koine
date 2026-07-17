# Instructions: Tests

**Applies to:** `crates/**/tests/**`, `#[cfg(test)]` modules, `tests/**`

- TDD: the failing test exists before the implementation. Commit history
  should show it (test and impl may share a commit; the test must fail
  without the impl).
- Name tests for the behavior: `expired_lease_makes_job_eligible_again`, not
  `test_lease_2`.
- Place tests at the innermost ring that can express the behavior
  ([testing-policy](../policies/testing-policy.md)): domain invariants in
  ring 1, use-case flows in ring 2 against `koine-store-memory`, adapter and
  crash/retry behavior in ring 3 against real Postgres with real migrations.
- Ring 3 uses testcontainers and `sqlx::migrate!` — never an inline schema
  duplicate.
- Every test asserts observable behavior, includes at least the relevant
  failure path, and would fail if the behavior regressed. Tests that mirror
  the implementation or assert nothing are review findings.
- Property tests (proptest) cover state machines; keep generators in the
  domain crate next to what they generate.
