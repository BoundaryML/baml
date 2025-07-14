// Test array types in BAML

class SimpleArrays {
  strings string[]
  integers int[]
  floats float[]
  booleans bool[]
}

class NestedArrays {
  matrix int[][]
  stringMatrix string[][]
  threeDimensional float[][][]
}

class ObjectArrays {
  users User[]
  products Product[]
  tags Tag[]
}

class User {
  id int
  name string
  email string
  isActive bool
}

class Product {
  id int
  name string
  price float
  tags string[]
  inStock bool
}

class Tag {
  id int
  name string
  color string
}

class MixedArrays {
  primitiveArray (string | int | float | bool)[]
  nullableArray (string | null)[]
  optionalItems (string?)[]
  arrayOfArrays string[][]
  complexMixed (User | Product | Tag)[]
}

class ArrayWithConstraints {
  nonEmptyStrings string[]
  limitedInts int[]
  positiveFloats float[]
}

function TestSimpleArrays(input: string) -> SimpleArrays {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a SimpleArrays object with:
    - strings: ["hello", "world", "test"]
    - integers: [1, 2, 3, 4, 5]
    - floats: [1.1, 2.2, 3.3]
    - booleans: [true, false, true, false]
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestNestedArrays(input: string) -> NestedArrays {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a NestedArrays object with:
    - matrix: [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
    - stringMatrix: [["a", "b"], ["c", "d"]]
    - threeDimensional: [[[1.0, 2.0], [3.0, 4.0]], [[5.0, 6.0], [7.0, 8.0]]]
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestObjectArrays(input: string) -> ObjectArrays {
  client "openai/gpt-4o-mini"
  prompt #"
    Return an ObjectArrays object with:
    - users: 3 user objects with different data
    - products: 2 product objects with realistic data
    - tags: 4 tag objects with different colors
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestMixedArrays(input: string) -> MixedArrays {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a MixedArrays object with:
    - primitiveArray: ["hello", 42, 3.14, true]
    - nullableArray: ["hello", null, "world", null]
    - optionalItems: ["present", null, "also present"]
    - arrayOfArrays: [["a", "b"], ["c", "d", "e"]]
    - complexMixed: mix of User, Product, and Tag objects

    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestEmptyArrays(input: string) -> SimpleArrays {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a SimpleArrays object with all empty arrays.
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestLargeArrays(input: string) -> SimpleArrays {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a SimpleArrays object with:
    - strings: array of 50 different words
    - integers: array of integers from 1 to 100
    - floats: array of 25 different decimal numbers
    - booleans: array of 20 alternating true/false values
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

// Top-level array return type tests
function TestTopLevelStringArray(input: string) -> string[] {
  client "openai/gpt-4o-mini"
  prompt #"
    Return an array of strings: ["apple", "banana", "cherry", "date"]
    
    Input: {{ input }}
  "#
}

function TestTopLevelIntArray(input: string) -> int[] {
  client "openai/gpt-4o-mini"
  prompt #"
    Return an array of integers: [10, 20, 30, 40, 50]
    
    Input: {{ input }}
  "#
}

function TestTopLevelFloatArray(input: string) -> float[] {
  client "openai/gpt-4o-mini"
  prompt #"
    Return an array of floats: [1.5, 2.5, 3.5, 4.5]
    
    Input: {{ input }}
  "#
}

function TestTopLevelBoolArray(input: string) -> bool[] {
  client "openai/gpt-4o-mini"
  prompt #"
    Return an array of booleans: [true, false, true, false, true]
    
    Input: {{ input }}
  "#
}

function TestTopLevelNestedArray(input: string) -> int[][] {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a 2D array of integers:
    [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
    
    Input: {{ input }}
  "#
}

function TestTopLevel3DArray(input: string) -> string[][][] {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a 3D array of strings:
    [[["a", "b"], ["c", "d"]], [["e", "f"], ["g", "h"]]]
    
    Input: {{ input }}
  "#
}

function TestTopLevelEmptyArray(input: string) -> string[] {
  client "openai/gpt-4o-mini"
  prompt #"
    Return an empty array: []
    
    Input: {{ input }}
  "#
}

function TestTopLevelNullableArray(input: string) -> (string | null)[] {
  client "openai/gpt-4o-mini"
  prompt #"
    Return an array with nullable strings: ["hello", null, "world", null, "!"]
    
    Input: {{ input }}
  "#
}

function TestTopLevelObjectArray(input: string) -> User[] {
  client "openai/gpt-4o-mini"
  prompt #"
    Return an array of 3 User objects with realistic data.
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestTopLevelMixedArray(input: string) -> (string | int | bool)[] {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a mixed type array: ["hello", 42, true, "world", 100, false]
    
    Input: {{ input }}
  "#
}

function TestTopLevelArrayOfMaps(input: string) -> map<string, int>[] {
  client "openai/gpt-4o-mini"
  prompt #"
    Return an array of maps:
    [{"a": 1, "b": 2}, {"x": 10, "y": 20}, {"foo": 100, "bar": 200}]
    
    Input: {{ input }}
  "#
}