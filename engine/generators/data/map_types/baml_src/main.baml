// Test map types in BAML

class SimpleMaps {
  stringToString map<string, string>
  stringToInt map<string, int>
  stringToFloat map<string, float>
  stringToBool map<string, bool>
  intToString map<string, string>
}

class ComplexMaps {
  userMap map<string, User>
  productMap map<string, Product>
  nestedMap map<string, map<string, string>>
  arrayMap map<string, int[]>
  mapArray map<string, string>[]
}

class User {
  id int
  name string
  email string
  active bool
}

class Product {
  id int
  name string
  price float
  tags string[]
}

class MixedKeyMaps {
  stringIntMap map<string, int>
  intStringMap map<string, string>
  // TODO: Fix Go code generation for enum and literal string keys
  // Should be: enumMap map<Status, string> but Go generator has issues with DecodeMap returning map[string]T instead of map[Status]T
  enumMap map<string, string>
  // Should be: literalMap map<"dev" | "staging" | "prod", Config> but Go generator has similar issues
  literalMap map<string, Config>
}

enum Status {
  ACTIVE
  INACTIVE
  PENDING
}

class Config {
  url string
  port int
  debug bool
}

class NestedMaps {
  simple map<string, string>
  oneLevelNested map<string, map<string, int>>
  twoLevelNested map<string, map<string, map<string, bool>>>
  mapOfArrays map<string, string[]>
  mapOfMaps map<string, map<string, float>>
}

class EdgeCaseMaps {
  emptyMap map<string, string>
  nullableValues map<string, string | null>
  optionalValues map<string, string?>
  unionValues map<string, string | int | bool>
}

function TestSimpleMaps(input: string) -> SimpleMaps {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a SimpleMaps object with:
    - stringToString: {"key1": "value1", "key2": "value2"}
    - stringToInt: {"one": 1, "two": 2, "three": 3}
    - stringToFloat: {"pi": 3.14159, "e": 2.71828}
    - stringToBool: {"isTrue": true, "isFalse": false}
    - intToString: {"1": "one", "2": "two", "3": "three"}
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestComplexMaps(input: string) -> ComplexMaps {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a ComplexMaps object with:
    - userMap: map with 2 users keyed by username
    - productMap: map with 3 products keyed by product id as strings
    - nestedMap: {"outer1": {"inner1": "value1", "inner2": "value2"}}
    - arrayMap: {"numbers": [1, 2, 3], "primes": [2, 3, 5, 7]}
    - mapArray: array of 2 different maps
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestNestedMaps(input: string) -> NestedMaps {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a NestedMaps object with various levels of nesting.
    Include at least 2 entries at each level.
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestEdgeCaseMaps(input: string) -> EdgeCaseMaps {
  client "openai/gpt-4o-mini"
  prompt #"
    Return an EdgeCaseMaps object with:
    - emptyMap: {}
    - nullableValues: {"present": "value", "absent": null}
    - optionalValues: {"required": "value", "optional": null}
    - unionValues: {"string": "hello", "number": 42, "boolean": true}
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestLargeMaps(input: string) -> SimpleMaps {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a SimpleMaps object where each map has at least 20 entries.
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}