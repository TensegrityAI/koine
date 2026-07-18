//! Koiné in-memory driven adapter for tests: complete port implementations without I/O.

pub mod dispatcher;
pub mod store;
pub mod test_support;

pub use dispatcher::InMemoryDispatcher;
pub use store::InMemoryEventStore;
pub use test_support::{FixedClock, SeededIds};
