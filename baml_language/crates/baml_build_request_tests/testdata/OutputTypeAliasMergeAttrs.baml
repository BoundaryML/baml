// Test attribute merging.
type Currency = int @check(gt_ten, {{ this > 10 }})
type Amount = Currency @assert({{ this > 0 }})

class MergeAttrs {
    amount Amount @description("In USD")
}

// This should be allowed.
type MultipleAttrs = int @assert({{ this > 0 }}) @check(gt_ten, {{ this > 10 }})

function FnOutputTypeAliasMergeAttrs(money: int) -> MergeAttrs {
    client "openai/gpt-4o"
    prompt #"
      Return the given integer in the specified format:

      {{ money }}

      {{ ctx.output_format }}
    "#
}

test TestOutputTypeAliasMergeAttrs {
  functions [FnOutputTypeAliasMergeAttrs]
  args {
    money 100
  }
}
