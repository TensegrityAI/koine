# Formal models

`lease_protocol.tla` is a TLC-checked model of Koiné's lease/delivery
protocol for **one job**. It mirrors `koine-domain`'s `Job` state machine
(`crates/koine-domain/src/job.rs` transition table): lease identity, expiry,
late acks, and the attempt cap. Multi-job/queue ordering is out of scope —
that's covered by the ring-3/ring-4 tests, not this model.

## Checked properties

TLC (`docs/formal/lease_protocol.cfg`) checks four invariants and one
liveness property over every reachable state:

- `TypeOK` — every variable stays within its declared type/range.
- `NoDualLease` — a live lease exists whenever the job is `leased` or
  `running`.
- `FreshLeases` — a lease id is never reused (`activeLease <= issued`).
- `AttemptCapped` — the attempt counter never exceeds `MaxAttempts`.
- `EventuallySettled` (liveness, under fairness of `Lease` and `Expire`) —
  the job always reaches a terminal state (`succeeded`/`cancelled`) or
  parks; it never pends forever.

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
`Expire`/`Cancel`, so bounding it cannot remove a path relevant to whether
the job's lifecycle state eventually settles — confirmed by the
bound-invariance check above.

## Drift rule

`job.rs`'s transition table and this model ship in the same PR. If a future
change to `Job::apply`, `Job::lease/start/succeed/fail/expire_lease/cancel`,
or the retry/attempt-cap logic changes the lease protocol's transitions,
update `lease_protocol.tla`/`.cfg` in that same change and re-run `make
tla`. If TLC later finds a counterexample in behavior phase 1 already
shipped, that's a phase-1 fidelity finding, not a phase-2 regression.
