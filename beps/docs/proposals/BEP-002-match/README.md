---
id: BEP-002
title: "match"
shepherds: hellovai <vbv@boundaryml.com>
status: Accepted
created: 2025-12-01
feedback: https://gloo-global.slack.com/docs/T03KV1PH19P/F0A1715PM52
---

The `match` expression in BAML provides a powerful way to handle different data shapes, particularly useful for working with union types and structured LLM outputs. It lets you write declarative, type-safe code that is easier to read and maintain than complex `if-else` chains.

## Quick Start

Use `match` to handle all possible values of an enum or union type:

```baml
enum Status {
  Active
  Inactive
  Pending
}

function GetMessage(s: Status) -> string {
  return match (s) {
    Status.Active => "User is active"
    Status.Inactive => "User is inactive"
    Status.Pending => "User is pending"
  }
}
```

The compiler ensures you handle every case. If you add a `Status.Archived` variant later, BAML will tell you exactly where you need to update your code.

## Core Concepts

### How Match Works

A `match` expression compares a value against a series of **patterns**. The first pattern that matches determines the result.

```baml
match (value) {
  Pattern1 => Result1
  Pattern2 => Result2
}
```

Because `match` is an **expression**, it evaluates to a value that you can assign to a variable or return directly.

### Type Patterns

When working with Union types (like `string | Image`), use the `variable: Type` syntax to narrow the type:

```baml
class Image { url string }

function GetContent(input: string | Image) -> string {
  return match (input) {
    // Matches if input is a string, binding it to 's'
    s: string => s
    
    // Matches if input is an Image, binding it to 'img'
    img: Image => img.url
  }
}
```

This is syntactically sugar for:
```javascript
if (input instanceof string) { ... }
else if (input instanceof Image) { ... }
```

### Wildcards

Use `_` (underscore) or a named variable to catch "everything else":

```baml
match (x) {
  1 => "One"
  other => "Something else: " + other
}
```

If you don't care about the value, you can just use `_`:

```baml
match (status) {
  Status.Active => "Active"
  _ => "Not active" 
}
```

## Common Patterns

### Handling LLM Outputs

BAML functions often return unions when the LLM might return different structures (e.g., a valid result or an error explanation).

```baml
class Success { data string }
class Failure { reason string }

function Process(result: Success | Failure) -> string {
  return match (result) {
    s: Success => "Computed: " + s.data
    f: Failure => "Failed: " + f.reason
  }
}
```

### Literal Matching

You can match exact values (strings, numbers, booleans) directly:

```baml
type Command = "start" | "stop" | int

match (cmd) {
  "start" => "Starting engine..."
  "stop" => "Stopping engine..."
  
  // Catch all integers
  seconds: int => "Sleeping for " + seconds + " seconds"
}
```

### Destructuring

You can unpack classes and objects directly in the pattern to access their fields:

```baml
class User {
  name string
  age int
}

function Greet(u: User) -> string {
  return match (u) {
    // Match specific field value
    User { name: "Admin" } => "Welcome, Administrator"
    
    // Bind 'name' variable
    User { name } => "Hello, " + name
  }
}
```

## Advanced Features

### Guards

Add conditions to patterns using `if`. The pattern matches only if the condition is true.

```baml
match (response) {
  // Pattern + Guard
  s: Success if s.score > 0.9 => "High confidence"
  
  // Fallback for same type
  s: Success => "Low confidence"
  
  Failure => "Failed"
}
```

### Complex Unions

You can match against subsets of a union.

```baml
type Primitive = string | int
type Complex = User | Image
type Any = Primitive | Complex

function Handle(val: Any) -> string {
  return match (val) {
    // Matches if val is string OR int
    p: Primitive => "Got a primitive"
    
    // Matches if val is User OR Image
    c: Complex => "Got an object"
  }
}
```

### Blocks

If you need to perform multiple actions in a match arm, use a block `{ ... }`. The last expression in the block is the result.

```baml
match (status) {
  Status.Error => {
    log("Error occurred")
    metrics.increment("errors")
    "Failed" // Return value
  }
  _ => "OK"
}
```

## Why `match`?

### 1. Type Safety (Exhaustiveness)
The biggest benefit is **exhaustiveness checking**. When you use `if/else`, it's easy to miss a case, especially when a type definition changes (e.g., adding a new Enum variant). With `match`, the BAML compiler guarantees you've handled every possible case.

### 2. Expressiveness
Parsing LLM outputs often involves checking for many different structures ("Did it return the answer? Did it return a clarification request? Did it return an error?"). `match` flattens what would be a deep nested `if` structure into a linear, readable list of cases.

### 3. Declaration vs Control Flow
`match` encourages a "parse, don't validate" mindset. Instead of checking conditions imperatively (`if x.is_valid()`), you declare the shapes of data you expect to handle. This aligns well with BAML's philosophy of treating data structures as first-class schemas.
