//! Queue naming and dispatch priority.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// A validated queue name: 1–128 chars of `[a-z0-9._-]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct QueueName(String);

impl QueueName {
    /// Validates and wraps a queue name.
    ///
    /// # Errors
    ///
    /// Returns `DomainError::InvalidQueueName` if the name is empty, exceeds 128 bytes, or contains invalid characters.
    pub fn new(name: impl Into<String>) -> Result<Self, DomainError> {
        let name = name.into();
        if name.is_empty() {
            return Err(DomainError::InvalidQueueName { reason: "empty" });
        }
        if name.len() > 128 {
            return Err(DomainError::InvalidQueueName {
                reason: "longer than 128 bytes",
            });
        }
        let ok = name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
        if !ok {
            return Err(DomainError::InvalidQueueName {
                reason: "only [a-z0-9._-] allowed",
            });
        }
        Ok(Self(name))
    }

    /// The name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for QueueName {
    type Error = DomainError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<QueueName> for String {
    fn from(value: QueueName) -> Self {
        value.0
    }
}

impl fmt::Display for QueueName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Dispatch priority: higher claims first; equal priorities are FIFO.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Priority(pub i16);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_queue_names() {
        for name in ["default", "emails.high", "a", "x_1-2.z"] {
            assert!(QueueName::new(name).is_ok(), "{name} should be valid");
        }
    }

    #[test]
    fn rejects_invalid_queue_names() {
        let too_long = "q".repeat(129);
        for name in ["", "UPPER", "with space", "ünïcode", too_long.as_str()] {
            assert!(QueueName::new(name).is_err(), "{name:?} should be invalid");
        }
    }

    #[test]
    fn queue_name_deserialization_revalidates() {
        assert!(serde_json::from_str::<QueueName>("\"ok-queue\"").is_ok());
        assert!(serde_json::from_str::<QueueName>("\"NOT OK\"").is_err());
    }

    #[test]
    fn priority_defaults_to_zero() {
        assert_eq!(Priority::default(), Priority(0));
    }
}
