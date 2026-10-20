// A declared hook is checked against `trace.Settings` and `trace.Options`
// by the identity of the `trace` package's declarations, not by resolving
// the name `trace` in the target's scope: a target whose generic parameter
// is named `trace` gets the same check as any other target.

function bad_hook(settings: trace.Settings) -> int throws never {
    1
}

/// baml:$trace=bad_hook
function target<trace>() -> int {
    1
}

/// baml:$trace=bad_hook
function plain_target() -> int {
    1
}
