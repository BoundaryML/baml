//! BCS cloud API contracts and native clients shared by BAML hosts.
//!
//! The `bcs_*` family owns cloud wire formats and HTTP operations. Telemetry
//! recording, bounded delivery and heartbeat scheduling belong to `btel_*`.

#[cfg(feature = "auth")]
pub mod auth;
pub mod credentials;
#[cfg(feature = "auth")]
pub mod error;
#[cfg(feature = "auth")]
pub mod query;
pub mod telemetry;
pub mod wire;

#[cfg(feature = "auth")]
pub use auth::{
    Caller, Client, DEFAULT_API_URL, DeviceLogin, Endpoint, HttpFailure, LoginPoll, Secret,
    Session, Store, StoredSession,
};
#[cfg(feature = "auth")]
pub use error::Error;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/btel.cloud.v1.rs"));
}
