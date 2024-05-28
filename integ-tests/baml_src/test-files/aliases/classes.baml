class TestClassAlias {
  key string @alias("key-dash")
  key2 string @alias("key21")
  key3 string @alias("key with space")
  key4 string //unaliased
  key5 string @alias("key.with.punctuation/123")
}

function FnTestClassAlias(input: string) -> TestClassAlias {
  client GPT35
  prompt #"
    {{ctx.output_format}}
  "#
}

test FnTestClassAlias {
  functions [FnTestClassAlias]
  args {
    input "example input"
  }
}
