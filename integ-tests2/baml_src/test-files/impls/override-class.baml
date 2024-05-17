class OverrideClass {
  prop1 string
  prop2 string
}

function FnTestClassOverride {
  input string
  output OverrideClass
}

impl<llm, FnTestClassOverride> v1 {
  client GPT35

  override OverrideClass {
    prop1 @alias("prop-one")
    prop2 @alias("prop-two")
  }
  prompt #"
    Return a json blob with made up fields using this schema:
    {#print_type(output)}

    JSON:
  "#
} 

