# Match Syntax Proposal

## Overview

This proposal introduces a **match expression** for BAML, enabling structural pattern matching on data types.

The core idea is to provide a type-safe, expressive way to handle different data shapes, particularly useful for working with union types and structured LLM outputs.

### Mental Model

`match` is an expression that takes a value and compares it against a series of patterns. The first matching pattern determines the result.

It is helpful to think of the `variable: Type` syntax as syntactic sugar for `if` statements with `instanceof` checks.

```baml
// This match expression:
match (val) {
  s: string => ...
  u: User => ...
}

// Is logically equivalent to:
if (val instanceof string) {
  let s = val; // narrowed to string
  ...
} else if (val instanceof User) {
  let u = val; // narrowed to User
  ...
}
```

```baml
let result = match (value) {
  Pattern1 => Result1
  Pattern2 => Result2
}
```

## Syntax Examples

### Basic Value Matching

```baml
enum Status {
  Active
  Inactive
  Pending
}

function StatusMessage(s: Status) -> string {
  return match (s) {
    Status.Active => "User is active"
    Status.Inactive => "User is inactive"
    Status.Pending => "User is pending"
  }
}
```

### Union Type Matching (Discriminated Unions)

BAML often deals with union types (e.g., `string | Image`). The syntax uses `variable: Type` for type patterns, consistent with variable declarations.

```baml
class Image {
  url string
}

function GetContent(input: string | Image) -> string {
  return match (input) {
    // Variable binding with Type Assertion
    s: string => s
    img: Image => img.url
  }
}
```

### Literal Matching in Unions

You can match exact values within a union without wrapping them.

```baml
type Result = "success" | int

match (res) {
  // Exact match (Literal)
  "success" => "Operation succeeded"
  
  // Type match (Catch-all for int)
  code: int => "Error code: " + code
}
```

### Destructuring

Match can destructure classes and objects using the existing object syntax.

```baml
class User {
  name string
  age int
}

function Greet(u: User) -> string {
  return match (u) {
    // Structural match with constant pattern
    User { name: "Admin" } => "Welcome, Administrator"
    
    // Structural match with guard
    User { name, age } if age < 18 => "Hello, young " + name
    
    // Structural match binding 'name'
    User { name } => "Hello, " + name
  }
}
```

## Design Rationale

### Primary Benefits

1.  **Type Safety**: The compiler ensures all cases are handled (exhaustiveness checking).
2.  **Expressiveness**: Concisely handle complex data structures without nested `if` statements.
3.  **LLM Output Handling**: Perfect for processing polymorphic outputs from LLMs (e.g., "Extract this, or return an error").

### Syntax Decisions

#### 1. Type Patterns: `var: Type`
**Decision**: Use `variable: Type` (e.g., `s: string`).
**Rationale**: This aligns with BAML's variable declaration syntax (`let s: string = ...`) and function arguments (`arg: Type`). It treats the pattern match as a "conditional declaration" of a variable with a specific type.
*   *Discarded Alternative*: `Type(var)` (Rust style) - Rejected because it looks like a constructor/function call, and primitives like `string` are not wrappers in BAML.

#### 2. Literal Matching
**Decision**: Use direct literals (`"abc"`, `123`).
**Rationale**: Simple and intuitive. No need to wrap them (e.g., `string("abc")` is redundant).

#### 3. Destructuring
**Decision**: Use `Type { field: pattern }`.
**Rationale**: Consistent with object construction syntax.

## Key Features

### 1. Exhaustiveness Checking

The compiler will error if not all possible cases are covered.

```baml
// Error: Missing case for Status.Pending
match (status) {
  Status.Active => ...
  Status.Inactive => ...
}
```

### 2. Guards

Add conditions to patterns using `if`.

```baml
match (response) {
  // Pattern + Guard
  s: Success if s.score > 0.9 => "High confidence"
  s: Success => "Low confidence"
  Failure => "Failed"
}
```

### 3. Wildcards

Use `_` or a named variable (without type) to catch "everything else".

```baml
match (x) {
  1 => "One"
  other => "Something else: " + other
}
```

## Advanced Matching

### 1. Subset Matching

Since `variable: Type` is just sugar for `if (variable instanceof Type)`, the type `T` doesn't have to be a single variant. It can be a subset of the union.

```baml
type Primitive = string | int | bool
type Complex = User | Image
type Any = Primitive | Complex

function Handle(val: Any) -> string {
  return match (val) {
    // Matches if val is string, int, or bool
    p: Primitive => "Got a primitive value"
    
    // Matches if val is User or Image
    c: Complex => "Got a complex object"
  }
}
```

### 2. Wildcard Binding

You can use `_` to match without binding a name, or `_: Type` to match a type without a name.

```baml
match (val) {
  // Ignore the value, just match the type
  _: int => "It's an integer"
  
  // Catch-all wildcard
  _ => "Everything else"
}
```

## Semantics: Value vs Type Patterns

The syntax unifies two kinds of matching:

1.  **Value Patterns**: Matching against a specific runtime value (Literals, Enum Members).
    *   Example: `"abc"`, `123`, `Status.Active`
2.  **Type Patterns**: Matching against a type variant in a Union.
    *   Example: `s: string`, `u: User`

### Mixed Matching Example

```baml
type Mixed = "Special" | string | Status

match (val) {
  // Value Patterns (Specific)
  "Special" => "Got the special string"
  Status.Active => "Got active status"
  
  // Type Patterns (General)
  // 's' is bound as 'Status' here (narrowed from Status.Active)
  s: Status => "Got some other status"
  
  // 'str' is bound as 'string'
  str: string => "Got some other string: " + str
}
```
