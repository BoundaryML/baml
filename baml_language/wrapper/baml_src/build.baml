// Build settings replaced in the temporary project by baml_language/wrapper/build.
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
