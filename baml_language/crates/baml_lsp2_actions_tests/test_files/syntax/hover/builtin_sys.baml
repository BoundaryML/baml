// Test hover on sys.shell builtin

function TestShell() -> string {
    baml.sys<[CURSOR].shell("echo hello")
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at hover_builtin_sys.baml:4:13
// No hover content
