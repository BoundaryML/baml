//! Host-side values the VM holds by reference: a host's values
//! ([`HostValueArc`]) and the footprint of what a resource buffers
//! ([`RetainedBytes`]).

pub mod host_value;
pub mod retained;
pub use host_value::{HostKey, HostReleaseFn, HostValueArc, HostValueKind, host_release_dispatch};
pub use retained::RetainedBytes;
