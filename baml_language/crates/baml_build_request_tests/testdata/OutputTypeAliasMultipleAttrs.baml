// This should be allowed.
type MultipleAttrs = int @assert({{ this > 0 }}) @check(gt_ten, {{ this > 10 }})

function FnOutputTypeAliasMultipleAttrs(money: int) -> MultipleAttrs {
    client "openai/gpt-4o"
    prompt #"
      Return the given integer without additional context:

      {{ money }}

      {{ ctx.output_format }}
    "#
}

test TestOutputTypeAliasMultipleAttrs {
  functions [FnOutputTypeAliasMultipleAttrs]
  args {
    money 100
  }
}
