//! Koiné data plane driving adapter: worker fetch stream, ack/fail, heartbeats, checkpoints over `gRPC`.

use koine_application as _;
use koine_domain as _;
