// Replaced in the build's temporary project by baml_language/wrapper/build.
// Direct source execution is not a packed release and has no baked compiler.
const WRAPPER_APP_VERSION = "development";
const WRAPPER_BAML_VERSION = "unpacked";

function wrapper_version() -> string {
    WRAPPER_APP_VERSION
}

function self_update_enabled() -> bool {
    true
}

// Pack builds take these from `wrapper/release.toml`.
// Direct development execution leaves telemetry disabled.
function telemetry_api_key() -> string {
    ""
}

function telemetry_default_host() -> string {
    "https://us.i.posthog.com"
}
