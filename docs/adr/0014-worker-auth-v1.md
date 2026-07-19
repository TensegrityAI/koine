# 0014 — Worker auth v1

- **Status:** accepted
- **Date:** 2026-07-19
- **Context:** Ratified scope (maintainer decision 2026-07-18): the data
  plane must not ship unauthenticated, and auth added after the v1 contract
  freeze would be breaking. v1 needs a minimal credible scheme, not an
  identity platform.
- **Decision:**
  - **Single shared bearer token per deployment**: server reads
    `KOINE_WORKER_TOKEN`; every RPC (including the Fetch stream's initial
    call) must carry metadata `authorization: Bearer <token>` — enforced by
    a tonic interceptor; failures return `UNAUTHENTICATED` with no detail
    leakage.
  - **Worker identity is claimed, not proven, in v1**: metadata
    `koine-worker-id`, validated as a domain `WorkerId` (non-empty, ≤256
    bytes, no control chars) — it scopes leases and presence, not
    privileges. All authenticated workers are equal.
  - **Transport security is proxy-terminated in v1**: deploy behind
    TLS-terminating ingress (documented guidance); the server binds plain
    HTTP/2. Native rustls and mTLS are explicitly NOT claimed.
  - Constant-time token comparison (subtle crate or length-guarded ct_eq)
    to avoid trivial timing oracles.
- **Consequences:** one secret to rotate (rotation = restart, acceptable
  v1); a leaked token grants full worker capability — documented; per-worker
  tokens/mTLS/OIDC become a phase-4+ backlog item with real requirements.
- **Alternatives considered:** no auth (fails the ratified scope); per-worker
  static tokens (secret sprawl without a management plane); mTLS (operational
  cost before any multi-tenant need); OIDC (an identity platform, not v1).
