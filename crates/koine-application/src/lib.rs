//! Koiné application layer: use cases and driven ports (`EventStore`, `Dispatcher`, `Clock`, `IdGenerator`).

pub mod lineage;
pub mod ports;
pub mod use_cases;

pub use lineage::{Lineage, lineage_of, wrap_events};
pub use ports::{
    Clock, DispatchError, Dispatcher, EventSink, EventStore, EventStoreError, IdGenerator,
    LeasedJob, RelayError, SinkError,
};
