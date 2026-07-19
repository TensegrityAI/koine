---- MODULE lease_protocol ----
(* Phase 2A: checked model of Koiné's lease/delivery protocol for ONE job.
   Mirrors koine-domain's Job state machine (job.rs transition table).
   Scope: lease identity, expiry, late acks, attempt cap, and the
   retryable/non-retryable fail split (job.rs Job::fail()). Atomicity note:
   each action models one database transaction (ADR 0011/0012) — the
   SKIP LOCKED claim is atomic BY CONSTRUCTION here; the implementation's
   obligation is exactly that atomicity. Multi-job/queue ordering is out of
   scope (covered by ring-3/ring-4 tests).

   Fix round 1 (review findings on this task): the original invariant set
   had no lease-fencing teeth (a weakened ack guard or a broken Expire
   reset both still passed TLC) and AckFail had no non-retryable path.
   Added ghost variables recording the fencing relation and the
   retryable/parked outcome at ack time (never read by any guard — pure
   observation), plus a quiescence invariant and a retryable/non-retryable
   split in AckFail. See docs/formal/README.md. *)

EXTENDS Naturals, FiniteSets

CONSTANTS Workers, MaxAttempts, MaxLeases, MaxConflicts

VARIABLES
    state, attempt, activeLease, issued, conflicts,
    \* Ghost state (Fix round 1): records facts about past actions purely so
    \* properties can check them. No action's guard ever reads these.
    lastAckLease, lastAckActiveLease, lastFailRetryable, lastFailParked

vars == <<state, attempt, activeLease, issued, conflicts,
           lastAckLease, lastAckActiveLease, lastFailRetryable, lastFailParked>>

States == {"pending", "leased", "running", "succeeded", "parked", "cancelled"}
Terminal == {"succeeded", "cancelled"}
NoLease == 0

Init ==
    /\ state = "pending"
    /\ attempt = 0
    /\ activeLease = NoLease
    /\ issued = 0
    /\ conflicts = 0
    /\ lastAckLease = NoLease
    /\ lastAckActiveLease = NoLease
    /\ lastFailRetryable = TRUE   \* vacuous sentinel: no fail recorded yet
    /\ lastFailParked = TRUE      \* vacuous sentinel: keeps the invariant trivially true at Init

(* A worker claims the job: one atomic tx issues a fresh lease id. *)
Lease ==
    /\ state = "pending"
    /\ issued < MaxLeases
    /\ issued' = issued + 1
    /\ activeLease' = issued + 1
    /\ state' = "leased"
    /\ UNCHANGED <<attempt, conflicts, lastAckLease, lastAckActiveLease,
                    lastFailRetryable, lastFailParked>>

Start ==
    /\ state = "leased"
    /\ state' = "running"
    /\ UNCHANGED <<attempt, activeLease, issued, conflicts, lastAckLease,
                    lastAckActiveLease, lastFailRetryable, lastFailParked>>

(* Ack with the CURRENT lease: normal completion. Records the fencing
   relation (the presented lease id vs. the lease that was actually active)
   at the moment the guard is supposed to enforce they match — see
   LeaseFencingOK. *)
AckSucceed(l) ==
    /\ state = "running" /\ l = activeLease
    /\ state' = "succeeded"
    /\ activeLease' = NoLease
    /\ lastAckLease' = l
    /\ lastAckActiveLease' = activeLease
    /\ UNCHANGED <<attempt, issued, conflicts, lastFailRetryable, lastFailParked>>

(* Ack failure. Mirrors job.rs's Job::fail(): attempt always increments (the
   Failed event always carries attempt+1 and is applied before the retry
   decision, in the same transaction — see job.rs ~line 356-383); a
   RETRYABLE error parks only once the attempt cap is hit, a NON-RETRYABLE
   error parks IMMEDIATELY regardless of attempt count. `retryable` is a
   nondeterministic environment input, like the lease id — the model
   doesn't choose it, the (mocked) job outcome does. Also records the
   fencing relation, same as AckSucceed, plus the retryable/parked outcome
   for NonRetryableAlwaysParks. *)
AckFail(l, retryable) ==
    LET nextState == IF retryable
                      THEN IF attempt + 1 >= MaxAttempts THEN "parked" ELSE "pending"
                      ELSE "parked"
    IN
        /\ state = "running" /\ l = activeLease
        /\ attempt' = attempt + 1
        /\ activeLease' = NoLease
        /\ state' = nextState
        /\ lastAckLease' = l
        /\ lastAckActiveLease' = activeLease
        /\ lastFailRetryable' = retryable
        /\ lastFailParked' = (nextState = "parked")
        /\ UNCHANGED <<issued, conflicts>>

(* Sweep: the lease deadline passed. *)
Expire ==
    /\ state \in {"leased", "running"}
    /\ attempt' = attempt + 1
    /\ activeLease' = NoLease
    /\ state' = IF attempt + 1 >= MaxAttempts THEN "parked" ELSE "pending"
    /\ UNCHANGED <<issued, conflicts, lastAckLease, lastAckActiveLease,
                    lastFailRetryable, lastFailParked>>

(* A STALE ack (lease no longer active): recorded as a conflict event,
   lifecycle state untouched — spec §3 "information is never lost". Not a
   lifecycle-changing ack (by definition l # activeLease here), so it must
   NOT touch the fencing ghost state. *)
LateAck(l) ==
    /\ l # activeLease /\ l >= 1 /\ l <= issued
    /\ conflicts' = conflicts + 1
    /\ UNCHANGED <<state, attempt, activeLease, issued, lastAckLease,
                    lastAckActiveLease, lastFailRetryable, lastFailParked>>

Cancel ==
    /\ state \in {"pending", "leased", "running", "parked"}
    /\ state' = "cancelled"
    /\ activeLease' = NoLease
    /\ UNCHANGED <<attempt, issued, conflicts, lastAckLease, lastAckActiveLease,
                    lastFailRetryable, lastFailParked>>

Next ==
    \/ Lease \/ Start \/ Expire \/ Cancel
    \/ \E l \in 1..MaxLeases : AckSucceed(l) \/ LateAck(l)
    \/ \E l \in 1..MaxLeases : \E retryable \in BOOLEAN : AckFail(l, retryable)

Spec == Init /\ [][Next]_vars /\ WF_vars(Lease) /\ WF_vars(Expire)

----
(* PROPERTIES *)

TypeOK ==
    /\ state \in States
    /\ attempt \in 0..MaxAttempts
    /\ activeLease \in 0..MaxLeases
    /\ issued \in 0..MaxLeases
    /\ conflicts \in Nat
    /\ lastAckLease \in 0..MaxLeases
    /\ lastAckActiveLease \in 0..MaxLeases
    /\ lastFailRetryable \in BOOLEAN
    /\ lastFailParked \in BOOLEAN

(* At most one live lease ever exists — by construction each Lease retires
   the notion of eligibility until Expire/AckFail return the job to pending,
   and activeLease is a single register. *)
NoDualLease == (state \in {"leased", "running"}) => activeLease # NoLease

(* A lease id is never reused. *)
FreshLeases == activeLease <= issued

(* Late acks never corrupt lifecycle state: proven structurally by LateAck's
   UNCHANGED clause; TypeOK + the state machine make it checkable. *)
AttemptCapped == attempt <= MaxAttempts

(* Fix round 1 — Finding 1 (fencing teeth). AckSucceed/AckFail record the
   presented lease id and the then-active lease id in the very step whose
   guard is supposed to force them equal. A guard regression (e.g.
   `l = activeLease` weakened to `l >= 1`) lets a step reach here with a
   mismatch, which this invariant then catches as a real state-space
   violation instead of relying on the guard text alone. *)
LeaseFencingOK == lastAckLease = lastAckActiveLease

(* Fix round 1 — Finding 1 (quiescence). activeLease is meaningful only
   while a lease is actually held (leased/running); every other state must
   show no active lease. Together with NoDualLease this pins
   `activeLease # NoLease` to be logically equivalent to
   `state \in {"leased", "running"}`. Catches a mutated Expire (or
   AckSucceed/AckFail/Cancel) that forgets to clear activeLease. *)
NoLeaseWhenIdle == (state \notin {"leased", "running"}) => activeLease = NoLease

(* Fix round 1 — Finding 2. job.rs's Job::fail() parks IMMEDIATELY on a
   non-retryable error, at ANY attempt count — it does not wait for the
   attempt cap. lastFailParked is recorded from the exact `nextState`
   AckFail computes, so if a mutation makes the non-retryable branch fall
   through to cap-only logic (i.e. ignores `retryable` and always applies
   "IF attempt+1>=MaxAttempts THEN parked ELSE pending"), a reachable
   non-retryable failure below the cap records lastFailRetryable=FALSE
   with lastFailParked=FALSE, and this invariant fails. *)
NonRetryableAlwaysParks == lastFailRetryable \/ lastFailParked

(* Liveness (under fairness of Lease and Expire): the job always reaches a
   terminal state or parks — no livelock where it pends forever. *)
EventuallySettled == <>[](state \in Terminal \cup {"parked"})

(* State-space bound (deviation from the phase-2a task brief's verbatim
   text, disclosed in docs/formal/README.md and the task report): the raw
   model has conflicts \in Nat, and LateAck increments it without bound,
   so the reachable state space is infinite and TLC would never terminate.
   MaxConflicts caps LateAck's guard so TLC can finish exploring; the
   modeled semantics (a late ack never changes lifecycle state) are
   unchanged — this only stops counting once the bound is hit. *)
StateConstraint == conflicts <= MaxConflicts
====
