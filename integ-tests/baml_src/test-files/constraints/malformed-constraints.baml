class MalformedConstraints {
  foo int @check(foo_check, {{ this.length() > 0 }})
}

class MalformedConstraints2 {
  foo int @assert(foo_check, {{ this.length() > 0 }})
}

function ReturnMalformedConstraints(a: int) -> MalformedConstraints {
  client GPT35
  prompt #"
    Return the integer after {{ a }}

    {{ ctx.output_format }}
  "#
}

function UseMalformedConstraints(a: MalformedConstraints2) -> int {
  client GPT35
  prompt #"
    Return the integer after {{ a.foo }}

    {{ ctx.output_format }}
  "#
}
