# Security Policy

## Supported versions

Pre-release: only the latest commit on `main` is supported.

## Reporting a vulnerability

Please report vulnerabilities privately via GitHub Security Advisories
("Report a vulnerability" on the repository's Security tab). Do **not** open a
public issue. You will receive an acknowledgement within 72 hours.

Scope notes for reporters: Koiné is a job broker — deserialization of untrusted
payloads, authentication of the data plane and control plane, and multi-tenant
queue isolation are the areas of highest interest.
