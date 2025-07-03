// Test optional and nullable types in BAML

class OptionalFields {
  requiredString string
  optionalString string?
  requiredInt int
  optionalInt int?
  requiredBool bool
  optionalBool bool?
  optionalArray string[]?
  optionalMap map<string, string>?
}

class NullableTypes {
  nullableString string | null
  nullableInt int | null
  nullableFloat float | null
  nullableBool bool | null
  nullableArray string[] | null
  nullableObject User | null
}

class User {
  id int
  name string
  email string?
  phone string | null
}

class MixedOptionalNullable {
  // Required field
  id int
  
  // Optional field (may not be present)
  description string?
  
  // Nullable field (present but can be null)
  metadata string | null
  
  // Optional and nullable
  notes string? | null
  
  // Arrays
  tags string[]  // required, can be empty
  categories string[]?  // optional
  keywords string[] | null  // nullable
  
  // Nested
  primaryUser User
  secondaryUser User?
  tertiaryUser User | null
}

class ComplexOptional {
  data OptionalData?
  items OptionalItem[]
  mapping map<string, OptionalValue?>
}

class OptionalData {
  value string
  count int?
  enabled bool?
}

class OptionalItem {
  id int
  name string
  description string?
  metadata map<string, string>?
}

class OptionalValue {
  data string | int | null
  optional string?
}

class UnionWithNull {
  simpleUnion string | int
  nullableUnion string | int | null
  optionalUnion (string | int)?
  complexUnion (User | Product | null)
}

class Product {
  id int
  name string
  price float?
}

function TestOptionalFields(input: string) -> OptionalFields {
  client "openai/gpt-4o-mini"
  prompt #"
    Return an OptionalFields object with:
    - requiredString: "hello"
    - optionalString: "world"
    - requiredInt: 42
    - optionalInt: null (omitted)
    - requiredBool: true
    - optionalBool: false
    - optionalArray: ["a", "b", "c"]
    - optionalMap: null (omitted)
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestNullableTypes(input: string) -> NullableTypes {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a NullableTypes object with:
    - nullableString: "present"
    - nullableInt: null
    - nullableFloat: 3.14
    - nullableBool: null
    - nullableArray: ["item1", "item2"]
    - nullableObject: null
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestMixedOptionalNullable(input: string) -> MixedOptionalNullable {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a MixedOptionalNullable object demonstrating different combinations:
    - Some optional fields present, others omitted
    - Some nullable fields with values, others null
    - Mix of empty and populated arrays
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestAllNull(input: string) -> NullableTypes {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a NullableTypes object with all fields set to null.
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestAllOptionalOmitted(input: string) -> OptionalFields {
  client "openai/gpt-4o-mini"
  prompt #"
    Return an OptionalFields object with:
    - All required fields with values (use 124 for int values, true for bool values, etc)
    - All optional fields omitted (not present)
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}