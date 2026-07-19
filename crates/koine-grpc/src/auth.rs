//! Bearer-token + worker-identity extraction (ADR 0014).

use koine_domain::WorkerId;
use subtle::ConstantTimeEq as _;
use tonic::Status;
use tonic::metadata::MetadataMap;

/// Validates `authorization: Bearer <token>` (constant-time) and the
/// `koine-worker-id` header. Returns the caller's `WorkerId`.
///
/// # Errors
/// `UNAUTHENTICATED` on any missing/invalid credential — no detail leakage.
pub fn check(metadata: &MetadataMap, expected_token: &str) -> Result<WorkerId, Status> {
    let unauthenticated = || Status::unauthenticated("invalid credentials");
    let auth = metadata
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(unauthenticated)?;
    let presented = auth.strip_prefix("Bearer ").ok_or_else(unauthenticated)?;
    let token_ok = presented.len() == expected_token.len()
        && bool::from(presented.as_bytes().ct_eq(expected_token.as_bytes()));
    if !token_ok {
        return Err(unauthenticated());
    }
    let worker = metadata
        .get("koine-worker-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(unauthenticated)?;
    WorkerId::new(worker).map_err(|_| unauthenticated())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "s3cr3t-token";

    fn metadata(auth: Option<&str>, worker_id: Option<&str>) -> MetadataMap {
        let mut metadata = MetadataMap::new();
        if let Some(auth) = auth {
            metadata.insert("authorization", auth.parse().expect("ascii metadata value"));
        }
        if let Some(worker_id) = worker_id {
            metadata.insert(
                "koine-worker-id",
                worker_id.parse().expect("ascii metadata value"),
            );
        }
        metadata
    }

    #[test]
    fn valid_pair_is_ok() {
        let metadata = metadata(Some(&format!("Bearer {TOKEN}")), Some("worker-1"));
        let worker = check(&metadata, TOKEN).expect("valid credentials");
        assert_eq!(worker.as_str(), "worker-1");
    }

    #[test]
    fn wrong_token_same_length_is_unauthenticated() {
        // Same length as TOKEN so the comparison exercises the ct_eq path
        // rather than short-circuiting on the length check.
        let wrong = "x".repeat(TOKEN.len());
        assert_ne!(wrong, TOKEN);
        let metadata = metadata(Some(&format!("Bearer {wrong}")), Some("worker-1"));
        let err = check(&metadata, TOKEN).expect_err("wrong token");
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
        assert_eq!(err.message(), "invalid credentials");
    }

    #[test]
    fn wrong_token_different_length_is_unauthenticated() {
        let metadata = metadata(Some("Bearer short"), Some("worker-1"));
        let err = check(&metadata, TOKEN).expect_err("wrong token");
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn missing_authorization_header_is_unauthenticated() {
        let metadata = metadata(None, Some("worker-1"));
        let err = check(&metadata, TOKEN).expect_err("missing header");
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn missing_worker_id_header_is_unauthenticated() {
        let metadata = metadata(Some(&format!("Bearer {TOKEN}")), None);
        let err = check(&metadata, TOKEN).expect_err("missing worker id");
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn empty_worker_id_is_unauthenticated() {
        let metadata = metadata(Some(&format!("Bearer {TOKEN}")), Some(""));
        let err = check(&metadata, TOKEN).expect_err("empty worker id");
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn control_char_worker_id_is_unauthenticated() {
        // Tab is a valid HTTP header-value byte (and so parses fine as
        // metadata) but is still a Rust `char::is_control` control
        // character, so this exercises `WorkerId::new`'s rejection rather
        // than the metadata layer's.
        let metadata = metadata(Some(&format!("Bearer {TOKEN}")), Some("worker\t1"));
        let err = check(&metadata, TOKEN).expect_err("control char worker id");
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
        assert_eq!(err.message(), "invalid credentials");
    }

    #[test]
    fn missing_bearer_prefix_is_unauthenticated() {
        let metadata = metadata(Some(TOKEN), Some("worker-1"));
        let err = check(&metadata, TOKEN).expect_err("missing bearer prefix");
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }
}
