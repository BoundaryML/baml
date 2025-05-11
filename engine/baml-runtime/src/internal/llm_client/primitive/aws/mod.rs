mod aws_client;
mod custom_http_client;
pub(super) mod types;
#[cfg(target_arch = "wasm32")]
pub(super) mod wasm;

pub use aws_client::AwsClient;
