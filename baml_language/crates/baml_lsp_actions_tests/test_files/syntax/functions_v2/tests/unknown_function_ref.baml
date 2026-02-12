// Test that unknown function references in test blocks produce errors

function RealFunction(input: string) -> string {
  client "openai/gpt-4"
  prompt #"Test function"#
}

test TestWithUnknown {
  functions [RealFunction, FakeFunction]
  args {
    input "hello"
  }
}

test TestAllUnknown {
  functions [NobodyHome]
  args {
    input "hello"
  }
}

//----
//- diagnostics
// Error: Unknown function `FakeFunction` in test block
//    ╭─[ functions_v2_tests_unknown_function_ref.baml:9:28 ]
//    │
//  9 │   functions [RealFunction, FakeFunction]
//    │                            ──────┬─────  
//    │                                  ╰─────── no function named `FakeFunction` exists
//    │ 
//    │ Note: Error code: E0088
// ───╯
// Error: Unknown function `NobodyHome` in test block
//     ╭─[ functions_v2_tests_unknown_function_ref.baml:16:14 ]
//     │
//  16 │   functions [NobodyHome]
//     │              ─────┬────  
//     │                   ╰────── no function named `NobodyHome` exists
//     │ 
//     │ Note: Error code: E0088
// ────╯
