# Phase 0 — Foundations

- **State:** done
- **Exit criterion:** full CI green from first push — **met** (run #1, id 29562780898, all 7 jobs success: fmt, clippy, test, docs, deny, typos, gitleaks)
- **Plan:** docs/superpowers/plans/2026-07-17-koine-phase-0-foundations.md
- **Delivered:** 11-crate workspace with compiled hexagonal boundaries, hygiene tooling
  (rustfmt/deny/typos/markdownlint/editorconfig), legal/community files (Apache-2.0),
  AGENTS.md + CLAUDE.md + `.apptlas/` AOL with DoD policy, ADRs 0001–0009, git hooks +
  conventional-commit gate + `make ci`, GitHub Actions CI (7 jobs).
- **Process:** subagent-driven development — 8 tasks, each with independent spec+quality
  review; 2 adjudicated fix rounds (gitleaks OSS CLI swap, merge/revert gate exemption);
  final whole-branch review APPROVED with pre-merge fix commit.
- **Follow-ups opened:** `todo/manifest-cleanup-workspace-deps.md`, `todo/ci-supply-chain-pinning.md`.

## 2026-07-22 follow-up closure amendment

The line above is the historical phase-0 closeout record. Both follow-ups are
now closed with checked acceptance evidence and faithful spec statements:

- [Centralize internal deps and clean manifest descriptions](manifest-cleanup-workspace-deps.md)
- [Decide CI action pinning and typos version policy](ci-supply-chain-pinning.md)
