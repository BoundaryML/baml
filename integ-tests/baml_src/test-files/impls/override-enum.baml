enum OverrideEnum {
  ONE
  TWO
}

function FnTestEnumOverride {
  input string
  output OverrideEnum
} 

impl<llm, FnTestEnumOverride> v1 { 
  client GPT35

// TODO: we should emit error diagnostic if override is unrelated to the function (like if we override a class here)
// TODO: also emit an error if overriding a value that doesnt exit
  override OverrideEnum {
    ONE @alias("key1")
    TWO @alias("key2")
  }
  prompt #"
    Return a random value from this list:
    {#print_enum(OverrideEnum)}

    Value:
  "#
}

