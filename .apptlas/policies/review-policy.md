# Policy: Review

Every task-closing review returns **two verdicts** — approving one does not
imply the other:

- **Spec compliance**: ✅/❌ — the work implements its requirements, nothing
  missing, nothing extra. Scored with
  [../rubrics/spec-fidelity-rubric.md](../rubrics/spec-fidelity-rubric.md).
- **Quality**: Approved / issues by severity. Scored with
  [../rubrics/code-review-rubric.md](../rubrics/code-review-rubric.md)
  (and [../rubrics/docs-quality-rubric.md](../rubrics/docs-quality-rubric.md)
  for documentation changes).

## Severity taxonomy

| Severity | Meaning | Effect |
| --- | --- | --- |
| **Critical** | Breaks correctness, delivery guarantees, security, or data integrity | Blocks merge; fix and re-review |
| **Important** | Real defect, DoD violation, or spec divergence without disposition | Blocks task closure; fix and re-review |
| **Minor** | Improvement or debt worth recording | Does not block — but MUST be recorded as a backlog finding or ledger line. A minor silently dropped is a review failure |

## Ground rules

- **Independence**: the reviewer is never the implementer. Fresh-context
  subagent reviewers count and are the default.
- **Evidence over trust**: reviewers reproduce key claims (run the command,
  read the file, download the artifact) rather than accepting the
  implementer's report — proportional to the claim's risk.
- **Re-review after fixes**: fixes to Critical/Important findings return to
  the same reviewer (or an equivalent one) with the fix diff and test
  evidence. No self-certified fixes.
- **Plan/spec conflicts escalate**: a finding that contradicts what the plan
  or spec mandates is the maintainer's decision — present both sides, don't
  silently pick one.
- **No pre-judged reviews**: dispatch prompts never tell a reviewer what not
  to flag or pre-rate a finding's severity.
