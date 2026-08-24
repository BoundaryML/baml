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
// E0003
//
//   × unresolved name: `baml.ev.send`
//    ╭─[expr_unresolved_builtin_namespace.baml:4:5]
//  4 │     baml.ev.send("test", {})
//    ·     ────────────
//    ╰────
// E0155
//
//   × type annotations needed
//   │ full type: `map<string, unknown>`
//    ╭─[expr_unresolved_builtin_namespace.baml:4:26]
//  4 │     baml.ev.send("test", {})
//    ·                          ──
//    ╰────
// E0003
//
//   × unresolved name: `baml.lo.info`
//    ╭─[expr_unresolved_builtin_namespace.baml:8:5]
//  8 │     baml.lo.info("test")
//    ·     ────────────
//    ╰────
// E0003
//
//   × unresolved name: `log.inf`
//     ╭─[expr_unresolved_builtin_namespace.baml:12:5]
//  12 │     log.inf("test")
//     ·     ───────
//     ╰────
// E0003
//
//   × unresolved name: `baml.event.send`
//     ╭─[expr_unresolved_builtin_namespace.baml:16:5]
//  16 │     baml.event.send("test", {})
//     ·     ───────────────
//     ╰────
// E0155
//
//   × type annotations needed
//   │ full type: `map<string, unknown>`
//     ╭─[expr_unresolved_builtin_namespace.baml:16:29]
//  16 │     baml.event.send("test", {})
//     ·                             ──
//     ╰────
