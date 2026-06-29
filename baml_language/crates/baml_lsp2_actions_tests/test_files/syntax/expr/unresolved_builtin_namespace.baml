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
//    ╭─[ expr_unresolved_builtin_namespace.baml:4:5 ]
//    │
//  4 │     baml.ev.send("test", {})
//    │     ──────┬─────
//    │           ╰─────── unresolved name: ev
//    │
//    │ Note: Error code: E0003
// ───╯
// error: unresolved name: lo
//    ╭─[ expr_unresolved_builtin_namespace.baml:8:5 ]
//    │
//  8 │     baml.lo.info("test")
//    │     ──────┬─────
//    │           ╰─────── unresolved name: lo
//    │
//    │ Note: Error code: E0003
// ───╯
// error: unresolved name: inf
//     ╭─[ expr_unresolved_builtin_namespace.baml:12:5 ]
//     │
//  12 │     log.inf("test")
//     │     ───┬───
//     │        ╰───── unresolved name: inf
//     │
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: event
//     ╭─[ expr_unresolved_builtin_namespace.baml:16:5 ]
//     │
//  16 │     baml.event.send("test", {})
//     │     ───────┬───────
//     │            ╰───────── unresolved name: event
//     │
//     │ Note: Error code: E0003
// ────╯
