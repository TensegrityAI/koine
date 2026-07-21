# Formal models

`lease_protocol.tla` is a TLC-checked model of Koiné's lease/delivery
protocol for **one job**. It mirrors `koine-domain`'s `Job` state machine
(`crates/koine-domain/src/job.rs` transition table): lease identity, expiry,
heartbeat renewal, explicit time/deadlines, late acks, the attempt cap, and the
retryable/non-retryable fail split.
Multi-job/queue ordering is out of scope — that's covered by the
ring-3/ring-4 tests, not this model.

## Checked properties

TLC (`docs/formal/lease_protocol.cfg`) checks eight invariants and one
liveness property over every reachable state:

- `TypeOK` — every variable stays within its declared type/range.
- `NoDualLease` — a live lease exists whenever the job is `leased` or
  `running`.
- `FreshLeases` — a lease id is never reused (`activeLease <= issued`).
- `AttemptCapped` — the attempt counter never exceeds `MaxAttempts`.
- `LeaseFencingOK` — a lifecycle-changing ack (`AckSucceed`/`AckFail`) must
  present the lease id that is actually active. This is checked via ghost
  state, not just the action guard (see "Ghost variables" below), so a
  regression in the guard is caught as a real invariant violation instead
  of quietly relying on the guard text never changing.
- `NoLeaseWhenIdle` — `activeLease` is meaningful only while the job is
  `leased` or `running`; every other state must show `NoLease`. Together
  with `NoDualLease` this pins `activeLease # NoLease` to be logically
  equivalent to `state \in {"leased", "running"}` — a quiescence check.
- `NonRetryableAlwaysParks` — a non-retryable failure parks the job
  immediately, regardless of attempt count (see "Retryable vs.
  non-retryable fails" below), checked via ghost state recorded at the
  moment `AckFail` decides the next state.
- `HeartbeatExpiryFence` — when expiry retires the lease grant most recently
  extended by a heartbeat, its observed time is at or after that heartbeat's
  accepted deadline. A stale expiry decision cannot revoke an accepted
  renewal.
- `EventuallySettled` (liveness, under weak fairness of `Lease`, `Heartbeat`,
  `Tick`, and `Expire`) — once the finite heartbeat allowance is exhausted or
  unused, time advances and the job reaches a terminal state
  (`succeeded`/`cancelled`) or parks; it never pends forever.

## Time, deadlines, and bounded renewal

`Lease` assigns `deadline = now + LeaseTtl`. `Tick` advances `now` while a
live lease is before its deadline, `Heartbeat` extends that current live lease
from `now`, and `Expire` is enabled only when `now >= deadline`. Every action
that ends a live lease resets `deadline` to zero.

`MaxHeartbeats = 2` and `LeaseTtl = 2` are model-checking bounds, not product
limits. The finite heartbeat count is also the explicit environment assumption
behind `EventuallySettled`: a worker may legitimately keep a lease forever in
the real protocol by renewing forever, so unconditional settlement would be a
false guarantee. With renewal bounded, weak fairness for `Heartbeat`, `Tick`,
and `Expire` explores renewal/time/retirement interleavings and ensures that
time cannot remain frozen before expiry.

## Ghost variables

The first four invariants above are checkable straight from `state`,
`attempt`, `activeLease`, and `issued`. `LeaseFencingOK` and
`NonRetryableAlwaysParks` are not — they're about a *relationship at the
moment of a transition* (did the presented lease match the active one? did
a non-retryable failure actually park?), and a plain safety invariant over
the post-transition state can't see that relationship once the transition
has happened. The model adds ghost variables to make these relationships
checkable:

- `lastAckLease`, `lastAckActiveLease` — on every `AckSucceed(l)` or
  `AckFail(l, retryable)`, record the presented lease id `l` and the
  `activeLease` value at that same step. `LeaseFencingOK` asserts they're
  always equal. Under the real guard (`l = activeLease`) this is trivially
  true; if the guard is ever weakened, a reachable step can record a
  mismatch and the invariant fails.
- `lastFailRetryable`, `lastFailParked` — on every `AckFail(l, retryable)`,
  record the `retryable` flag and whether the computed next state was
  `"parked"`. `NonRetryableAlwaysParks == lastFailRetryable \/
  lastFailParked` asserts that whenever the recorded failure was
  non-retryable, it parked. Both are initialized to `TRUE` (a vacuous
  sentinel meaning "no failure recorded yet"), so the invariant is
  trivially satisfied before the first `AckFail`.
- `lastHeartbeatLease`, `lastHeartbeatDeadline` — record the grant and the
  deadline accepted by the latest `Heartbeat`.
- `lastExpiredLease`, `lastExpiryNow` — record the grant and model time of the
  latest `Expire`. `HeartbeatExpiryFence` compares these observations without
  making them inputs to an action guard.

The lease/boolean observations are bounded and the time observations inherit
the finite lease/heartbeat bounds, so they do not affect TLC's termination.
No action's guard reads them; they exist purely so properties can observe
facts about past transitions that the plain state variables do not retain.

## Retryable vs. non-retryable fails

`job.rs`'s `Job::fail()` (~line 356-383) always increments `attempt` (the
`Failed` event always carries `attempt + 1`, applied before the retry
decision, in the same transaction), then branches on `error.retryable`:
retryable errors get the normal retry-or-park-at-cap decision: parked only
once `attempt >= MaxAttempts`; a non-retryable error parks *immediately*,
at any attempt count, with no cap check at all.

The model's `AckFail(l, retryable)` takes `retryable` as a second,
nondeterministic parameter (like the lease id — the model doesn't choose
it, the mocked job outcome does) and mirrors the branch exactly:

```text
nextState == IF retryable
             THEN IF attempt + 1 >= MaxAttempts THEN "parked" ELSE "pending"
             ELSE "parked"
```

`attempt' = attempt + 1` unconditionally either way, matching `job.rs`.
`Next` quantifies over both lease id and `retryable \in BOOLEAN`, so both
paths are explored. `TypeOK`'s domain for `attempt` is unaffected — the
non-retryable path can only park on that lease cycle, then the job has no
running state left to fail again (parked is a dead end here; `Repaired` is
out of scope, see below), so it can't push `attempt` past `MaxAttempts`.

## Running it

```sh
make tla
```

Downloads `tla2tools.jar` into `docs/formal/.tools/` (gitignored) on first
run, then invokes TLC against `lease_protocol.cfg`/`lease_protocol.tla`.
Expected output ends with:

```text
Model checking completed. No error has been found.
```

With the checked configuration (`MaxLeases = 5`, `MaxHeartbeats = 2`,
`LeaseTtl = 2`, and the existing attempt/conflict bounds), TLC generates
74,079 states, finds 18,598 distinct states, leaves zero states queued, and
reaches graph depth 24.

`make tla` is a separate gate from `make ci` (TLC needs a JVM, which the
plain Rust CI jobs don't otherwise pull in). CI runs it as its own job,
`tla`, in `.github/workflows/ci.yml` (after `markdownlint`), using
`actions/setup-java@v4` (Temurin 21).

## Scope

Single-job model only. `Workers` is declared as a constant for a future
multi-worker refinement, but no current action indexes on it (TLC accepts
the unused constant). States in scope: `pending`, `leased`, `running`,
`succeeded`, `parked`, `cancelled` — the phase-5-reserved `suspended` /
`awaiting_approval` states and the operator-triggered `repaired` transition
are not modeled here; they aren't part of the lease protocol proper.

Heartbeat identity is represented by the active lease grant. `Heartbeat` is
enabled only while that grant is live and unexpired; expiry clears it, so a
retirement-first serialization makes a later heartbeat unavailable, while a
heartbeat-first serialization moves the current deadline and fences expiry
until that deadline.

## Deviations from a naive transcription of the transition table

Two additions to `lease_protocol.cfg` (not the module's actions/invariants)
were needed to make TLC terminate cleanly; neither changes the modeled
protocol semantics:

- **`conflicts` is unbounded in the module** (`conflicts \in Nat`, and
  `LateAck` increments it with no upper guard), so the raw reachable state
  space is infinite and TLC would never terminate. `StateConstraint ==
  conflicts <= MaxConflicts` (new constant, `MaxConflicts = 3` in the
  `.cfg`) bounds exploration via `CONSTRAINT StateConstraint`. This only
  stops TLC from counting late-ack conflicts past the bound — `LateAck`'s
  action body (and its `UNCHANGED` clause proving lifecycle state is
  untouched) is unmodified. Re-running with `MaxConflicts` at 1 and 6
  reproduces the same result (`No error has been found`, distinct-state
  count scaling linearly with the bound), confirming the bound isn't
  hiding anything: `conflicts` doesn't gate any lifecycle-affecting action.
- **`CHECK_DEADLOCK FALSE`** in the `.cfg`: `succeeded` and `cancelled` are
  genuine terminal states with no enabled `Next` action once no lease was
  ever issued (`issued = 0`), matching `job.rs` (nothing but a
  `LateAckConflict` — itself gated on `issued >= 1` — applies to a
  terminated job). TLC's default deadlock check flags "no enabled action"
  states as errors unless told otherwise; this tells it that's expected
  here, without touching any invariant or the liveness property.

TLC also warns, generically, that declaring a state constraint during
liveness checking can be unsound in general (constraints can prune paths a
liveness property depends on). It doesn't apply here: `conflicts` is
disjoint from every guard on `Lease`/`Start`/`AckSucceed`/`AckFail`/
`Expire`/`Heartbeat`/`Tick`/`Cancel`, so bounding it cannot remove a path
relevant to whether the job's lifecycle state eventually settles — confirmed
by the bound-invariance check above.

## Mutation-probe evidence (fix round 1)

The pre-fix invariant set (`TypeOK`, `NoDualLease`, `FreshLeases`,
`AttemptCapped`) passed TLC even under two mutations that broke the
protocol: weakening `AckSucceed`'s guard from `l = activeLease` to `l >= 1`,
and making `Expire` leave `activeLease` unchanged instead of clearing it.
Neither mutation touches a variable those four invariants inspect, so
neither was caught. `LeaseFencingOK` and `NoLeaseWhenIdle` were added
specifically to close that gap, and both are probe-verified:

- Guard-weakening mutation (`AckSucceed(l)`'s guard changed to `l >= 1`):
  TLC reports `Invariant LeaseFencingOK is violated` at search depth 4
  (`lastAckLease = 2`, `lastAckActiveLease = 1` in the failing state).
- `Expire` mutation (`activeLease' = NoLease` dropped, i.e. `activeLease`
  left `UNCHANGED`): TLC reports `Invariant NoLeaseWhenIdle is violated`
  at search depth 3 (`state = "pending"`, `activeLease = 1`).
- A third probe against `NonRetryableAlwaysParks` (collapsing
  `AckFail`'s branch back to cap-only logic, i.e. ignoring `retryable`):
  TLC reports `Invariant NonRetryableAlwaysParks is violated` at search
  depth 4 (`lastFailRetryable = FALSE`, `lastFailParked = FALSE`,
  `state = "pending"`).

All three are one-line, revertible edits to `lease_protocol.tla`, applied
and reverted for this check — the fixed module in this repo does not
contain them.

## Heartbeat-expiry mutation probe

Before fencing `Expire` on the current deadline, the action was deliberately
given the stale/early-class guard `now >= deadline - LeaseTtl`. `make tla`
then reported `Invariant HeartbeatExpiryFence is violated` at graph depth 4
along the shortest trace `Init -> Lease -> Heartbeat -> Expire`. This minimal
witness is specifically **early-after-accepted-heartbeat**: `Lease` and
`Heartbeat` both execute at `now = 0`, so the accepted heartbeat deadline
remains `2` rather than moving, yet lease `1` is retired at `now = 0`. The
mutant represents the broader stale/early defect class because its guard
ignores the current deadline; this shortest witness does not claim to show a
displaced deadline. The guard was replaced with `now >= deadline`; under the
same constants, invariants, fairness, liveness property, and bounds, TLC
completed with no error and the state counts recorded above.

## Drift rule

`job.rs`'s transition table and this model ship in the same PR. If a future
change to `Job::apply`, `Job::lease/start/succeed/fail/expire_lease/cancel`,
or the retry/attempt-cap logic changes the lease protocol's transitions,
update `lease_protocol.tla`/`.cfg` in that same change and re-run `make
tla`. If TLC later finds a counterexample in behavior phase 1 already
shipped, that's a phase-1 fidelity finding, not a phase-2 regression.
