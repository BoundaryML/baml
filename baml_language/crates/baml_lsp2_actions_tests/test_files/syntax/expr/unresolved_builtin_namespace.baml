// Test that invalid baml namespace paths produce diagnostics

function TestBamlEv() -> void {
    baml.ev.send("test", {})
}

function TestBamlLo() -> void {
    baml.lo.info("test")
}

function TestLogInf() -> void {
    log.inf("test")
}

function TestBamlEventsTypo() -> void {
    baml.event.send("test", {})
}

//----
//- diagnostics
// error: unresolved name: ev
//    ╭─[ expr_unresolved_builtin_namespace.baml:3:32 ]
//    │
//  3 │ ╭─▶ function TestBamlEv() -> void {
//  4 │ ├─▶     baml.ev.send("test", {})
//    │ │                                  
//    │ ╰────────────────────────────────── unresolved name: ev
//    │     
//    │     Note: Error code: E0003
// ───╯
// error: unresolved name: lo
//    ╭─[ expr_unresolved_builtin_namespace.baml:7:32 ]
//    │
//  7 │ ╭─▶ function TestBamlLo() -> void {
//  8 │ ├─▶     baml.lo.info("test")
//    │ │                              
//    │ ╰────────────────────────────── unresolved name: lo
//    │     
//    │     Note: Error code: E0003
// ───╯
// error: unresolved name: inf
//     ╭─[ expr_unresolved_builtin_namespace.baml:11:32 ]
//     │
//  11 │ ╭─▶ function TestLogInf() -> void {
//  12 │ ├─▶     log.inf("test")
//     │ │                         
//     │ ╰───────────────────────── unresolved name: inf
//     │     
//     │     Note: Error code: E0003
// ────╯
// error: unresolved name: event
//     ╭─[ expr_unresolved_builtin_namespace.baml:15:40 ]
//     │
//  15 │ ╭─▶ function TestBamlEventsTypo() -> void {
//  16 │ ├─▶     baml.event.send("test", {})
//     │ │                                     
//     │ ╰───────────────────────────────────── unresolved name: event
//     │     
//     │     Note: Error code: E0003
// ────╯
