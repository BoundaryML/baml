type Primitive = int | string | bool | float

function FnOutputTypeAliasPrimitive(p: Primitive) -> Primitive {
    client "openai/gpt-4o"
    prompt #"
      Return the given value back: {{ p }}

      {{ ctx.output_format }}
    "#
}

test TestOutputTypeAliasPrimitive {
  functions [FnOutputTypeAliasPrimitive]
  args {
    p 42
  }
}
