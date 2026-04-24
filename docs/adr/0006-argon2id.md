# ADR-0006: Argon2id for password hashing

**Status:** Accepted
**Date:** 2026-04-24

## Context

Ferro ships baked-in authentication. The password hash function is load-bearing for account security.

## Decision

Use **Argon2id** via the `argon2` crate, with parameters following OWASP 2023+ recommendations (m=19 MiB, t=2, p=1 as a floor; tune per deploy).

## Rationale

- Argon2id won the PHC competition and is the current OWASP first-choice recommendation.
- Memory-hard → resists GPU/ASIC attacks better than bcrypt/scrypt at equivalent CPU cost.
- `argon2` crate is pure Rust, audited, zeroize-friendly.

## Alternatives Considered

- **bcrypt**: Still acceptable; slower wall-clock progress, not memory-hard.
- **scrypt**: Memory-hard but parameter selection is more awkward; less momentum than argon2.
- **PBKDF2**: Only if FIPS compliance demands it; inferior otherwise.

## Consequences

- Verifying passwords allocates ~19 MiB per attempt — this is intentional and bounded by login rate limits.
- Parameters are stored in the encoded hash so we can raise them over time without breaking existing users.
