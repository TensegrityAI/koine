# 0001 — Koiné identity and name

- **Status:** accepted
- **Date:** 2026-07-16

- **Context:** The project's original name, NEXUS, was dropped due to a hard collision with Sonatype Nexus. "Rosetta" was considered as a replacement and dropped in turn, given the collision with Apple's Rosetta and the Rosetta Stone trademark. The project needed an identity that carries its core thesis rather than a merely functional label.

- **Decision:** Name the project **Koiné** (κοινή, "the common [language]"). The name is the thesis: the koiné was the shared language that let speakers of many dialects work together; Koiné is the common language between programming languages for background work. The core thesis it encodes: the history of every job is the source of truth, not a byproduct. `koine` was free on crates.io as of 2026-07-16.

- **Consequences:**
  - Easier: name and thesis are the same statement — explaining "why Koiné" doubles as explaining the architecture's core commitment (total traceability, repair & resume, agent-native operation all follow from "job history as source of truth").
  - Harder: crates.io names (`koine`, `koine-*`) must be reserved before first publish to avoid squatting.
  - Gave up: any brand equity already built around "NEXUS" from the 2025 draft and PoC.

- **Alternatives considered:**
  - NEXUS — rejected: hard collision with Sonatype Nexus.
  - Rosetta — rejected: collision with Apple Rosetta and the Rosetta Stone trademark.
  - Telar, Relevo, Bitácora — considered during the same naming pass and set aside in favor of Koiné, which states the project's thesis directly.
