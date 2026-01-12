// Test edge cases in BAML

class EmptyCollections {
  emptyStringArray string[]
  emptyIntArray int[]
  emptyObjectArray User[]
  emptyMap map<string, string>
  emptyNestedArray string[][]
}

class User {
  id int
  name string
}

class LargeStructure {
  // Test with many fields
  field1 string
  field2 string
  field3 string
  field4 string
  field5 string
  field6 int
  field7 int
  field8 int
  field9 int
  field10 int
  field11 float
  field12 float
  field13 float
  field14 float
  field15 float
  field16 bool
  field17 bool
  field18 bool
  field19 bool
  field20 bool
  // Arrays
  array1 string[]
  array2 int[]
  array3 float[]
  array4 bool[]
  array5 User[]
  // Maps
  map1 map<string, string>
  map2 map<string, int>
  map3 map<string, float>
  map4 map<string, bool>
  map5 map<string, User>
}

class DeepRecursion {
  value string
  next DeepRecursion?
}

class VeryLongStrings {
  shortString string
  mediumString string
  longString string
  veryLongString string
  extremelyLongString string
}

class SpecialCharacters {
  normalText string
  withNewlines string
  withTabs string
  withQuotes string
  withBackslashes string
  withUnicode string
  withEmoji string
  withMixedSpecial string
}

class NumberEdgeCases {
  zero int
  negativeInt int
  largeInt int
  veryLargeInt int @description(#"
    i64 max value
  "#)
  smallFloat float
  largeFloat float
  negativeFloat float
  scientificNotation float @description(#"
    > 1e3, but < 1e6
  "#)
  infinity float?
  notANumber float?
}

class BooleanEdgeCases {
  explicitTrue bool
  explicitFalse bool
  arrayOfTrue bool[]
  arrayOfFalse bool[]
  mixedBoolArray bool[]
}

class NullEdgeCases {
  allNull AllNullable
  someNull SomeNullable
  nestedNull NestedNullable
}

class AllNullable {
  nullString string | null
  nullInt int | null
  nullFloat float | null
  nullBool bool | null
  nullArray string[] | null
  nullObject User | null
}

class SomeNullable {
  presentString string | null
  nullString string | null
  presentInt int | null
  nullInt int | null
}

class NestedNullable {
  outer OuterNullable | null
}

class OuterNullable {
  inner InnerNullable | null
}

class InnerNullable {
  value string | null
}

class CircularReference {
  id int
  name string
  parent CircularReference?
  children CircularReference[]
  relatedItems CircularReference[]
}

class MixedEdgeCases {
  emptyString string
  singleChar string
  veryLongArray string[]
  deeplyNestedMap map<string, map<string, map<string, string>>>
  mixedTypeArray (string | int | bool | null)[]
  optionalEverything OptionalEverything?
}

class OptionalEverything {
  optString string?
  optInt int?
  optFloat float?
  optBool bool?
  optArray string[]?
  optMap map<string, string>?
  optObject User?
}

function TestEmptyCollections(input: string) -> EmptyCollections {
  client "openai/gpt-4o-mini"
  prompt #"
    Return an EmptyCollections object with all collections empty.
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestLargeStructure(input: string) -> LargeStructure {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a LargeStructure object with all fields populated with appropriate test data.
    Arrays should have 3-5 items each.
    Maps should have 2-3 entries each.
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestDeepRecursion(depth: int) -> DeepRecursion {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a DeepRecursion object nested to depth {{ depth }}.
    Each level should have value "Level X" where X is the level number.
    
    {{ ctx.output_format }}
    
    Input: {{ depth }}
  "#
}

function TestSpecialCharacters(input: string) -> SpecialCharacters {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a SpecialCharacters object with:
    - normalText: "Hello World"
    - withNewlines: "Line 1\nLine 2\nLine 3"
    - withTabs: "Column1\tColumn2\tColumn3"
    - withQuotes: "She said \"Hello\" to him"
    - withBackslashes: "C:\\Users\\Name\\Documents"
    - withUnicode: "Hello 世界 🌍"
    - withEmoji: "Happy 😊 Sad 😢 Love ❤️"
    - withMixedSpecial: "Mix: \n\t\"Hello\\World\" 🌟"
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestNumberEdgeCases(input: string) -> NumberEdgeCases {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a NumberEdgeCases object with various edge case numbers.
    Include zero, negatives, very large numbers, and scientific notation.
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestCircularReference(input: string) -> CircularReference {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a CircularReference object representing a tree structure:
    - Root node with id 1
    - Two children with ids 2 and 3
    - Child 2 has parent reference to node 1
    - Child 3 has parent reference to node 1
    - Some related items referencing other nodes
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}