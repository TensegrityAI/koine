# Epic: Phase 5 — Durable execution

- **State:** planned
- **Implements:** design spec §1 (repair & resume thesis), §3 (durable event
  kinds), §6 phase 5
- **Exit criteria:** an agentic job fails at step N, is repaired (payload/
  state edited with history preserved), and resumes from its last
  checkpoint — via API, CLI, MCP, and dashboard.

## Candidate items

1. **Checkpoint semantics** — `CheckpointRecorded` end-to-end: worker
   journals side-effect results; on retry/resume the SDK replays the journal
   so completed steps are not re-executed. ADR: checkpoint payload contract
   and size limits.
2. **TLA+ model extension** — checkpoint/resume added to the phase-2 model;
   new property: a journaled side effect is never re-executed after resume;
   TLC re-checked before implementation.
3. **Signals** — `SignalReceived` + wait points: a running/suspended job
   receives external input (data plane + control plane + MCP paths).
4. **Approvals (human-in-the-loop)** — `ApprovalRequested/Granted/Denied`;
   job suspends awaiting decision; approval surfaces in dashboard + MCP;
   audit trail is just… the event log.
5. **Suspend/resume** — explicit operator/agent-driven pause with lease
   handling defined (no lease burn while suspended).
6. **Repair & resume — the killer feature** — `JobRepaired`: edit
   payload/state via control plane with full prior history retained; job
   re-enters eligible state continuing from last checkpoint. UX in CLI
   (`koine repair`), dashboard (park list → repair flow), MCP tool.
7. **Python SDK durable support** — checkpoint helper, idempotent step
   wrapper, signal/approval await API; conformance suite extended with
   durable scenarios. *(ring 4)*
8. **Docs** — `durable-execution.md` wiki chapter + a worked agent-pipeline
   example (the 40-minute-agent-job story from the spec, made runnable).

## Dependencies

- Phase 4 (operational surfaces exist to exercise repair/approval flows);
  schema groundwork from phase 1 (event kinds reserved — no migration
  surgery expected; if surgery IS needed, that's a phase-1 fidelity finding).

## Risks

- Checkpoint replay semantics are where durable-execution systems get subtle
  (determinism boundaries) — the TLA+ extension and conformance scenarios
  come first, implementation second.
- Repair UX can silently violate append-only instincts — repairs are NEW
  events, never edits; reviewers hold this line (event-sourcing
  instructions).

## Verification strategy

TLC on the extended model; ring 1 for new state transitions (proptest set
extended); ring 4 durable conformance scenarios; the fail→repair→resume demo
is the acceptance instrument, run through all four surfaces.
