#![allow(
    clippy::disallowed_methods,
    reason = "a build script reads cargo's environment"
)]

fn main() {
    napi_build::setup();
    // Node-API functions are Node's: they resolve when Node loads the addon.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-undefined,dynamic_lookup");
    }
}
