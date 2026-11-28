//! The BCS client as BAML hosts use it: `bcs-sdk`, which BCS publishes, and what is BAML's own.
//! BAML reads its endpoint overrides and its home directory from the environment ([`endpoint`],
//! [`store`], and `services_endpoint` with the `cloud` feature) and words each failure for its
//! users ([`diagnostics`]); the client reads no environment of its own. The formats are
//! `btel-core`'s.
//!
//! Telemetry recording, bounded delivery and heartbeat scheduling belong to `btel_*`.

pub use bcs_sdk::{ApiErrorBody, HttpFailure, credentials, response, telemetry, upload};
#[cfg(feature = "auth")]
pub use bcs_sdk::{
    Caller, Client, DEFAULT_API_URL, DeviceLogin, Endpoint, Error, LoginPoll, Secret, Session,
    Store, StoredSession, api_keys, auth, error, query,
};
#[cfg(feature = "cloud")]
pub use bcs_sdk::{CloudClient, cloud};

#[cfg(feature = "auth")]
pub mod diagnostics;

/// The control plane of Boundary's own cloud, which serves the default API endpoint's logins.
#[cfg(feature = "cloud")]
pub const DEFAULT_SERVICES_URL: &str = "https://services.cloud.boundaryml.com";

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

/// The control plane BAML manages cloud resources through: `BOUNDARY_CLOUD_SERVICES_URL` when it
/// is set, else `configured`, else Boundary's own when `api` is Boundary's own. Another API
/// endpoint has no default: its login's access token must not reach a control plane it does not
/// belong to.
#[cfg(feature = "cloud")]
pub fn services_endpoint(configured: Option<&str>, api: &Endpoint) -> error::Result<Endpoint> {
    let override_url = baml_env::string_var("BOUNDARY_CLOUD_SERVICES_URL")
        .map_err(|_| Error::Endpoint("Boundary services URL is not valid Unicode"))?;
    match override_url.as_deref().or(configured) {
        Some(url) => Endpoint::parse(url).map_err(|_| {
            Error::Endpoint(
                "Boundary services URL must be an HTTPS base URL, or HTTP on loopback, without credentials, query or fragment",
            )
        }),
        None if api.as_str() == DEFAULT_API_URL => Endpoint::parse(DEFAULT_SERVICES_URL),
        None => Err(Error::Endpoint(
            "Boundary services URL is required when the API URL is not Boundary's own",
        )),
    }
}
