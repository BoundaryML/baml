// Test completions for log package and baml.events namespace

function TestLogCompletion() -> void {
    log.<[CURSOR]
}

//----
//- diagnostics
// error: Expected Expected field name after '.', found '}'
//    ╭─[ baml_namespace.baml:5:1 ]
//    │
//  5 │ }
//    │ ┬  
//    │ ╰── Expected Expected field name after '.', found '}'
//    │ 
//    │ Note: Error code: E0010
// ───╯
//
//- completions
// SHOULD_CONTAIN: info, debug, warn, error
