// Test hover on sys.shell builtin

function TestShell() -> baml.sys.ShellOutput {
    baml.sys<[CURSOR].shell("echo hello", null)
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at hover_builtin_sys.baml:4:13
// No hover content
