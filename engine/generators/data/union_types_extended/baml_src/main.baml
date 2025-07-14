// Test extended union types in BAML

class PrimitiveUnions {
  stringOrInt string | int
  stringOrFloat string | float
  intOrFloat int | float
  boolOrString bool | string
  anyPrimitive string | int | float | bool
}

class UnionArrays {
  mixedArray (string | int)[]
  nullableItems (string | null)[]
  objectArray (User | Product)[]
  nestedUnionArray (string | int[])[]
}

class User {
  id int
  name string
  type "user"
}

class Product {
  id int
  name string
  price float
  type "product"
}

class Admin {
  id int
  name string
  permissions string[]
  type "admin"
}

class ComplexUnions {
  userOrProduct User | Product
  userOrProductOrAdmin User | Product | Admin
  dataOrError DataResponse | ErrorResponse
  resultOrNull Result | null
  multiTypeResult Success | Warning | Error
}

class DataResponse {
  data string
  timestamp int
  status "success"
}

class ErrorResponse {
  error string
  code int
  status "error"
}

class Result {
  value string | int | float
  metadata map<string, string>
}

class Success {
  type "success"
  message string
  data map<string, string>
}

class Warning {
  type "warning"
  message string
  level int
}

class Error {
  type "error"
  message string
  code int
  details string?
}

class DiscriminatedUnions {
  shape Circle | Rectangle | Triangle
  animal Dog | Cat | Bird
  response ApiSuccess | ApiError | ApiPending
}

class Circle {
  shape "circle"
  radius float
}

class Rectangle {
  shape "rectangle"
  width float
  height float
}

class Triangle {
  shape "triangle"
  base float
  height float
}

class Dog {
  species "dog"
  breed string
  goodBoy bool
}

class Cat {
  species "cat"
  color string
  lives int
}

class Bird {
  species "bird"
  canFly bool
  wingspan float?
}

class ApiSuccess {
  status "success"
  data map<string, string>
}

class ApiError {
  status "error"
  message string
  code int
}

class ApiPending {
  status "pending"
  progress float
  eta int?
}

class RecursiveUnion {
  value string | int | RecursiveUnion
  children (string | RecursiveUnion)[]
}

function TestPrimitiveUnions(input: string) -> PrimitiveUnions {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a PrimitiveUnions object with:
    - stringOrInt: 42 (as int)
    - stringOrFloat: "hello" (as string)
    - intOrFloat: 3.14 (as float)
    - boolOrString: true (as bool)
    - anyPrimitive: "mixed" (as string)
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestComplexUnions(input: string) -> ComplexUnions {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a ComplexUnions object with various union types populated.
    Mix between different types in the unions.
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestDiscriminatedUnions(input: string) -> DiscriminatedUnions {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a DiscriminatedUnions object with:
    - shape: a circle with radius 5.0
    - animal: a dog that is a golden retriever and is a good boy
    - response: an error with message "Not found" and code 404
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestUnionArrays(input: string) -> UnionArrays {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a UnionArrays object with:
    - mixedArray: ["hello", 1, "world", 2]
    - nullableItems: ["present", null, "also present", null]
    - objectArray: mix of User and Product objects
    - nestedUnionArray: ["string", [1, 2, 3], "another", [4, 5]]
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}