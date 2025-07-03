// Test all primitive types in BAML

class PrimitiveTypes {
  stringField string
  intField int
  floatField float
  boolField bool
  nullField null
}

class PrimitiveArrays {
  stringArray string[]
  intArray int[]
  floatArray float[]
  boolArray bool[]
}

class PrimitiveMaps {
  stringMap map<string, string>
  intMap map<string, int>
  floatMap map<string, float>
  boolMap map<string, bool>
}

class MixedPrimitives {
  name string
  age int
  height float
  isActive bool
  metadata null
  tags string[]
  scores int[]
  measurements float[]
  flags bool[]
}

function TestPrimitiveTypes(input: string) -> PrimitiveTypes {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a PrimitiveTypes object with the following values:
    - stringField: "Hello, BAML!"
    - intField: 42
    - floatField: 3.14159
    - boolField: true
    - nullField: null
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestPrimitiveArrays(input: string) -> PrimitiveArrays {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a PrimitiveArrays object with the following values:
    - stringArray: ["hello", "world", "baml"]
    - intArray: [1, 2, 3, 4, 5]
    - floatArray: [1.1, 2.2, 3.3, 4.4]
    - boolArray: [true, false, true, false]
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestPrimitiveMaps(input: string) -> PrimitiveMaps {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a PrimitiveMaps object with the following values:
    - stringMap: {"key1": "value1", "key2": "value2"}
    - intMap: {"one": 1, "two": 2, "three": 3}
    - floatMap: {"pi": 3.14159, "e": 2.71828}
    - boolMap: {"isTrue": true, "isFalse": false}
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestMixedPrimitives(input: string) -> MixedPrimitives {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a MixedPrimitives object with realistic test data.
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestEmptyCollections(input: string) -> PrimitiveArrays {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a PrimitiveArrays object with all empty arrays.
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}