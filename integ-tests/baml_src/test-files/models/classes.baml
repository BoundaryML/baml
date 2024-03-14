// Test overrides
class TestClassAlias {
  key string @alias("key-dash")
  key2 string @alias("key21")
  key3 string @alias("key with space")
  key4 string //unaliased
}

function FnTestClassAlias {
  input string
  output TestClassAlias
}

impl<llm, FnTestClassAlias> v1 {
  client GPT35
  prompt #"
    Return a json blob that has this schema:
    {#print_type(output)}
  "#
}

