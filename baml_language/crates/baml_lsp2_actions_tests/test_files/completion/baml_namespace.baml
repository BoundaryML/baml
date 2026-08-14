// Test completions for log package and baml.events namespace

function TestLogCompletion() -> void {
    log.<[CURSOR]
}

//----
//- diagnostics
// E0010
//
//   × unexpected token
//    ╭─[baml_namespace.baml:5:1]
//  5 │ }
//    · ┬
//    · ╰── expected `field name after `'.'``, found `'}'`
//    ╰────
//
//- completions
// SHOULD_CONTAIN: info, debug, warn, error
