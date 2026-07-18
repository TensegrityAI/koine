//! Identifier newtypes. `UUIDv7` by convention; generated only behind the
//! application `IdGenerator` port so the domain stays free of clocks and
//! randomness (ADR 0010).

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::DomainError;

macro_rules! uuid_newtype {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Wraps an existing UUID.
            #[must_use]
            pub const fn new(id: Uuid) -> Self {
                Self(id)
            }

            /// Returns the inner UUID.
            #[must_use]
            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

uuid_newtype!(
    /// Identity of a job — also its event-stream id.
    JobId
);
uuid_newtype!(
    /// Identity of a single recorded event.
    EventId
);
uuid_newtype!(
    /// Identity of one lease grant.
    LeaseId
);
uuid_newtype!(
    /// Correlates all events caused by one logical operation.
    CorrelationId
);

/// Identity a worker chooses for itself (validated string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WorkerId(String);

impl WorkerId {
    /// Validates and wraps a worker id: non-empty, ≤256 bytes, no control chars.
    ///
    /// # Errors
    ///
    /// Returns `DomainError::InvalidWorkerId` if the id is empty, exceeds 256 bytes, or contains control characters.
    pub fn new(id: impl Into<String>) -> Result<Self, DomainError> {
        let id = id.into();
        if id.is_empty() {
            return Err(DomainError::InvalidWorkerId { reason: "empty" });
        }
        if id.len() > 256 {
            return Err(DomainError::InvalidWorkerId {
                reason: "longer than 256 bytes",
            });
        }
        if id.chars().any(char::is_control) {
            return Err(DomainError::InvalidWorkerId {
                reason: "control characters",
            });
        }
        Ok(Self(id))
    }

    /// The id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for WorkerId {
    type Error = DomainError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<WorkerId> for String {
    fn from(value: WorkerId) -> Self {
        value.0
    }
}

impl fmt::Display for WorkerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn job_id_serde_is_transparent() {
        let id = JobId::new(Uuid::from_u128(7));
        let json = serde_json::to_string(&id).expect("serialize");
        assert_eq!(json, format!("\"{}\"", id.as_uuid()));
        let back: JobId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, id);
    }

    #[test]
    fn worker_id_rejects_empty_and_oversized() {
        assert!(WorkerId::new("").is_err());
        assert!(WorkerId::new("w".repeat(257)).is_err());
        assert!(WorkerId::new("worker-1").is_ok());
    }
}
