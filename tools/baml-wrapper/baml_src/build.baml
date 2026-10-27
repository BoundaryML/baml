// Replaced in the build's temporary project by tools/baml-wrapper/build.
function wrapper_version() -> string {
    "0.2.5"
}

function self_update_enabled() -> bool {
    true
}

// Pack builds derive these from the Rust wrapper's PostHog constants.
// Direct development execution leaves telemetry disabled.
function telemetry_api_key() -> string {
    ""
}

function telemetry_default_host() -> string {
    "https://us.i.posthog.com"
}
