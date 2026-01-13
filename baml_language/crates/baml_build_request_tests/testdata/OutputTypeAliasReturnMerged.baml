type Currency = int @check(gt_ten, {{ this > 10 }})
type Amount = Currency @assert({{ this > 0 }})

function FnOutputTypeAliasReturnMerged(money: int) -> Amount {
    client "openai/gpt-4o"
    prompt #"
      Return the given integer without additional context:

      {{ money }}

      {{ ctx.output_format }}
    "#
}

test TestOutputTypeAliasReturnMerged {
  functions [FnOutputTypeAliasReturnMerged]
  args {
    money 100
  }
}
