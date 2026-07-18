//! Koiné application layer: use cases and driven ports (`EventStore`, `OutboxRelay`, `ProjectionStore`, `LeaseManager`, `Clock`, `IdGenerator`).

pub mod lineage;
pub mod ports;

pub use lineage::{Lineage, wrap_events};
pub use ports::{
    Clock, DispatchError, Dispatcher, EventStore, EventStoreError, IdGenerator, LeasedJob,
};
