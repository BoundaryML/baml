// Test hover on sys.shell builtin

function TestShell() -> string {
    baml.sys<[CURSOR].shell("echo hello")
}

//----
//- diagnostics
// Error: unresolved name: baml
//    ╭─[ hover_builtin_sys.baml:3:33 ]
//    │
//  3 │ ╭─▶ function TestShell() -> string {
//  4 │ ├─▶     baml.sys.shell("echo hello")
//    │ │                                      
//    │ ╰────────────────────────────────────── unresolved name: baml
//    │     
//    │     Note: Error code: E0001
// ───╯
//
//- on_hover expressions
// hover at hover_builtin_sys.baml:4:13
// No hover content
