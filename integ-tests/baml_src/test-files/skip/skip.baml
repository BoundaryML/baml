// Test file for @skip attribute functionality
// GitHub issue #2915: @skip + @@dynamic causes panic
// Discord bug report: @skip on non-dynamic class causes panic

// Test 1: @skip with @@dynamic class (GitHub #2915)
class SkipDynamicClass {
  value string
  internal_id string? @skip
  @@dynamic
}

function TestSkipDynamic(input: string) -> SkipDynamicClass {
  client GPT35
  prompt #"
    Extract a value from the input:
    {{ input }}

    {{ ctx.output_format }}
  "#
}

// Test 2: @skip on non-dynamic class with optional fields (Discord bug report)
class SkipNonDynamicClass {
  name string
  description string?
  metadata string? @skip
}

function TestSkipNonDynamic(input: string) -> SkipNonDynamicClass {
  client GPT35
  prompt #"
    Extract info from the input:
    {{ input }}

    {{ ctx.output_format }}
  "#
}
