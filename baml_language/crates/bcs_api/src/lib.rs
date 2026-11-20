//! The BCS client as BAML hosts use it: `bcs-sdk`, which BCS publishes, and what is BAML's own.
//! BAML reads its endpoint override and its home directory from the environment ([`endpoint`],
//! [`store`]) and words each failure for its users ([`diagnostics`]); the client reads no
//! environment of its own. The formats are `btel-core`'s.
//!
//! Telemetry recording, bounded delivery and heartbeat scheduling belong to `btel_*`.

pub use bcs_sdk::{ApiErrorBody, HttpFailure, credentials, response, telemetry, upload};
#[cfg(feature = "auth")]
pub use bcs_sdk::{
    Caller, Client, DEFAULT_API_URL, DeviceLogin, Endpoint, Error, LoginPoll, Secret, Session,
    Store, StoredSession, api_keys, auth, error, query,
};

#[cfg(feature = "auth")]
pub mod diagnostics;

/// The endpoint BAML talks to: `BOUNDARY_API_URL` when it is set, else `configured`, else BCS's
/// own.
#[cfg(feature = "auth")]
pub fn endpoint(configured: Option<&str>) -> error::Result<Endpoint> {
    // Reading a string variable fails only when its value is not Unicode.
    let override_url = baml_env::string_var("BOUNDARY_API_URL")
        .map_err(|_| Error::Endpoint("BOUNDARY_API_URL is not valid Unicode"))?;
    Endpoint::resolve(override_url.as_deref(), configured)
}

/// The saved logins for `endpoint`, under `$BAML_HOME` (by default `~/.baml`), which every BAML
/// host shares.
#[cfg(feature = "auth")]
pub fn store(endpoint: &Endpoint) -> Store {
    let home = baml_env::baml_home_from(baml_env::os_var("BAML_HOME"), dirs::home_dir());
    Store::in_home(endpoint, &home)
}
