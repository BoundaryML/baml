// Test that unknown function references in test blocks produce errors

function RealFunction(input: string) -> string {
  client: "openai/gpt-4"
  prompt: `Test function`
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
// <no-diagnostics-expected>
