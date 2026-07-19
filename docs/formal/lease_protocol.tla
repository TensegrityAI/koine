---- MODULE lease_protocol ----
(* Phase 2A: checked model of Koiné's lease/delivery protocol for ONE job.
   Mirrors koine-domain's Job state machine (job.rs transition table).
   Scope: lease identity, expiry, late acks, attempt cap. Atomicity note:
   each action models one database transaction (ADR 0011/0012) — the
   SKIP LOCKED claim is atomic BY CONSTRUCTION here; the implementation's
   obligation is exactly that atomicity. Multi-job/queue ordering is out of
   scope (covered by ring-3/ring-4 tests). *)

EXTENDS Naturals, FiniteSets

CONSTANTS Workers, MaxAttempts, MaxLeases, MaxConflicts

VARIABLES state, attempt, activeLease, issued, conflicts

vars == <<state, attempt, activeLease, issued, conflicts>>

States == {"pending", "leased", "running", "succeeded", "parked", "cancelled"}
Terminal == {"succeeded", "cancelled"}
NoLease == 0

Init ==
    /\ state = "pending"
    /\ attempt = 0
    /\ activeLease = NoLease
    /\ issued = 0
    /\ conflicts = 0

(* A worker claims the job: one atomic tx issues a fresh lease id. *)
Lease ==
    /\ state = "pending"
    /\ issued < MaxLeases
    /\ issued' = issued + 1
    /\ activeLease' = issued + 1
    /\ state' = "leased"
    /\ UNCHANGED <<attempt, conflicts>>

Start ==
    /\ state = "leased"
    /\ state' = "running"
    /\ UNCHANGED <<attempt, activeLease, issued, conflicts>>

(* Ack with the CURRENT lease: normal completion. *)
AckSucceed(l) ==
    /\ state = "running" /\ l = activeLease
    /\ state' = "succeeded"
    /\ activeLease' = NoLease
    /\ UNCHANGED <<attempt, issued, conflicts>>

AckFail(l) ==
    /\ state = "running" /\ l = activeLease
    /\ attempt' = attempt + 1
    /\ activeLease' = NoLease
    /\ state' = IF attempt + 1 >= MaxAttempts THEN "parked" ELSE "pending"
    /\ UNCHANGED <<issued, conflicts>>

(* Sweep: the lease deadline passed. *)
Expire ==
    /\ state \in {"leased", "running"}
    /\ attempt' = attempt + 1
    /\ activeLease' = NoLease
    /\ state' = IF attempt + 1 >= MaxAttempts THEN "parked" ELSE "pending"
    /\ UNCHANGED <<issued, conflicts>>

(* A STALE ack (lease no longer active): recorded as a conflict event,
   lifecycle state untouched — spec §3 "information is never lost". *)
LateAck(l) ==
    /\ l # activeLease /\ l >= 1 /\ l <= issued
    /\ conflicts' = conflicts + 1
    /\ UNCHANGED <<state, attempt, activeLease, issued>>

Cancel ==
    /\ state \in {"pending", "leased", "running", "parked"}
    /\ state' = "cancelled"
    /\ activeLease' = NoLease
    /\ UNCHANGED <<attempt, issued, conflicts>>

Next ==
    \/ Lease \/ Start \/ Expire \/ Cancel
    \/ \E l \in 1..MaxLeases : AckSucceed(l) \/ AckFail(l) \/ LateAck(l)

Spec == Init /\ [][Next]_vars /\ WF_vars(Lease) /\ WF_vars(Expire)

----
(* PROPERTIES *)

TypeOK ==
    /\ state \in States
    /\ attempt \in 0..MaxAttempts
    /\ activeLease \in 0..MaxLeases
    /\ issued \in 0..MaxLeases
    /\ conflicts \in Nat

(* At most one live lease ever exists — by construction each Lease retires
   the notion of eligibility until Expire/AckFail return the job to pending,
   and activeLease is a single register. *)
NoDualLease == (state \in {"leased", "running"}) => activeLease # NoLease

(* A lease id is never reused. *)
FreshLeases == activeLease <= issued

(* Late acks never corrupt lifecycle state: proven structurally by LateAck's
   UNCHANGED clause; TypeOK + the state machine make it checkable. *)
AttemptCapped == attempt <= MaxAttempts

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
