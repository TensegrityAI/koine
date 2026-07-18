# Formal models

Draft TLA+ models co-evolving with the implementation. Status: **skeleton** —
`lease_protocol.tla` mirrors `koine-domain`'s `Job` state machine (phase 1A).
TLC model-checking, the per-lease identity needed for the dual-lease and
late-ack properties, and CI integration are phase-2 deliverables
(`.apptlas/epics/phase-2-data-plane.md`, item 1). If TLC later finds a
counterexample in behavior phase 1 already implements, that is a phase-1
fidelity finding (phase-2 epic, risks).
