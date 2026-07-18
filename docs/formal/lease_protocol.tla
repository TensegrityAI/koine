---- MODULE lease_protocol ----
(* DRAFT SKELETON — written alongside phase 1A's state machine so model and
   code co-evolve (phase-2 epic risk mitigation). TLC configuration and the
   checked properties land with phase 2's data plane. *)

EXTENDS Naturals

CONSTANTS Workers, MaxAttempts

VARIABLES state, attempt, holder

vars == <<state, attempt, holder>>

States == {"pending", "leased", "running", "succeeded", "parked", "cancelled"}

NoWorker == "none"

Init == state = "pending" /\ attempt = 0 /\ holder = NoWorker

Lease(w) ==
    /\ state = "pending"
    /\ state' = "leased" /\ holder' = w
    /\ UNCHANGED attempt

Start ==
    /\ state = "leased"
    /\ state' = "running"
    /\ UNCHANGED <<attempt, holder>>

Succeed ==
    /\ state = "running"
    /\ state' = "succeeded" /\ holder' = NoWorker
    /\ UNCHANGED attempt

Fail ==
    /\ state = "running"
    /\ attempt' = attempt + 1
    /\ holder' = NoWorker
    /\ state' = IF attempt + 1 >= MaxAttempts THEN "parked" ELSE "pending"

Expire ==
    /\ state \in {"leased", "running"}
    /\ attempt' = attempt + 1
    /\ holder' = NoWorker
    /\ state' = IF attempt + 1 >= MaxAttempts THEN "parked" ELSE "pending"

Cancel ==
    /\ state \in {"pending", "leased", "running", "parked"}
    /\ state' = "cancelled" /\ holder' = NoWorker
    /\ UNCHANGED attempt

Next == Start \/ Succeed \/ Fail \/ Expire \/ Cancel \/ (\E w \in Workers : Lease(w))

TypeOK ==
    /\ state \in States
    /\ attempt \in 0..MaxAttempts
    /\ holder \in Workers \cup {NoWorker}

(* Properties to check with TLC in phase 2 (needs per-lease identity added):
   - NoDualLease: two workers never hold a live lease simultaneously
   - NoLostJob: every job ends succeeded/parked/cancelled or stays reachable
   - LateAckSafety: a stale ack never changes lifecycle state *)
====
