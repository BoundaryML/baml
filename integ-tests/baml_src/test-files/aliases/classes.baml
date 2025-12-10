class TestClassAlias {
  key string @alias("key-dash") @description(#"
    This is a description for key
    af asdf
  "#)
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

// Test class for schema extraction with nested types
class AddressWithMeta {
  street string @alias("street_name") @description("The street name")
  city string @alias("city_name") @description("The city")
  zipcode string @description("5-digit zip code")
}

class PersonWithMeta {
  name string @alias("full_name") @description("Person's full legal name")
  age int @description("Age in years")
  address AddressWithMeta @description("Home address")
  tags string[] @alias("labels") @description("User tags")
}

function ExtractPersonWithMeta(input: string) -> PersonWithMeta {
  client GPT35
  prompt #"
    Extract person info: {{ input }}
    {{ ctx.output_format }}
  "#
}

test ExtractPersonWithMeta {
  functions [ExtractPersonWithMeta]
  args {
    input "test"
  }
}
