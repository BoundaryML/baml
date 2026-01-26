---
id: BEP-002
title: "match"
shepherds: hellovai <vbv@boundaryml.com>, rossirpaulo <rossir.paulo@gmail.com>
status: Accepted
created: 2025-12-15
---

The `match` expression in BAML provides exhaustive, type-safe pattern matching over union types, enums, and literal values. It enables declarative handling of different data shapes while ensuring at compile time that all cases are covered.

## Motivation

LLM responses in BAML frequently use union types and nullable fields. Currently, handling these requires verbose `if-else` chains that are error-prone and don't guarantee exhaustiveness:

```baml
function Process(result: LlmResult) -> string {
  if (result == null) {
    return "No result"
  }
  if (result instanceof Success) {
    if (result.score >= 0.9) {
      return "High confidence"
    }
    return "Low confidence"
  }
  if (result instanceof Failure) {
    return "Failed: " + result.reason
  }
  // What if we forget a case? No compiler help.
}
```

Pattern matching solves this with:
1. **Exhaustiveness checking** — the compiler ensures all cases are handled
2. **Cleaner syntax** — flat, declarative case enumeration
3. **Type narrowing** — bound variables have their type narrowed within each arm

## Core Design Decisions

### Decision 1: Type Patterns Require Explicit Binding

**Problem:** In a bare pattern like `match(x) { Type1 => ... }`, it's ambiguous whether `Type1` is a type name or a variable/value.

**Solution:** Type patterns always use the `name: TypeExpr` syntax:

```baml
match (x) {
  s: Success => "got success: " + s.data   // s is bound, type is Success
  _: Failure => "got failure"              // _ is bound but discarded
  value1 => "literal match"                // value1 is a literal or catch-all
}
```

This makes parsing unambiguous and aligns with TypeScript's type annotation syntax.

### Decision 2: `_` is a Binding, Not a Special Keyword

The underscore `_` is a valid binding name. Like any other binding, it captures the matched value. However, the value is **dropped later in the pipeline** (not accessible in the arm body).

```baml
match (result) {
  _: Success => "success"     // _ bound to Success, but dropped
  other => "other: " + other  // other bound and usable
}
```

**Important:** There is no special `default` keyword. Any untyped binding (including `_` or any identifier) acts as a catch-all because it matches any value.

### Decision 3: Catch-All via Untyped Binding

A pattern without `: TypeExpr` matches anything and binds the scrutinee:

```baml
match (x) {
  _: int => "integer"
  _: string => "string"
  other => "something else: " + other  // catch-all, binds to 'other'
}

// Or using _ as catch-all (value discarded):
match (x) {
  _: int => "integer"
  _ => "not an integer"  // catch-all, value discarded
}
```

### Decision 4: Full Type Expression Generality

The type expression after `:` supports full generality — unions, type aliases, parenthesized groups:

```baml
// All equivalent:
x: int | bool
x: (int | bool)
x: (int) | (bool)

// Complex unions:
result: Success | Failure
code: 200 | 201 | 204
cmd: "start" | "stop"
```

### Decision 5: Union Types Don't Collapse

When binding a union pattern, the bound variable retains the **exact union type**, not a collapsed/widened type:

```baml
class Success { data string }
class Failure { reason string }
class Pending { eta int }

match (result) {
  x: Success | Failure => {
    // x has type `Success | Failure`, NOT some supertype
    handle(x)
  }
  _: Pending => "pending"
}
```

This preserves precision: you know exactly which types `x` could be.

### Decision 6: Exhaustiveness via Untyped Binding

Exhaustiveness is required. An untyped binding (catch-all) satisfies exhaustiveness for all remaining cases:

```baml
type Result = Success | Failure | null

// Exhaustive via explicit patterns:
match (result) {
  _: Success => "ok"
  _: Failure => "error"
  null => "nothing"
}

// Exhaustive via catch-all:
match (result) {
  _: Success => "ok"
  _ => "not success"  // covers Failure and null
}

// NOT exhaustive — compile error:
match (result) {
  _: Success => "ok"
  _: Failure => "error"
  // Error: 'null' not handled
}
```

**Future enhancement:** Warn when a catch-all covers multiple cases, to encourage explicit handling.

## Syntax Specification

### Grammar

```
match_expr     := 'match' '(' expr ')' '{' match_arm+ '}'
match_arm      := pattern guard? '=>' arm_body
pattern        := binding_pattern | literal_pattern | union_pattern
binding_pattern := IDENT (':' type_expr)?
literal_pattern := 'null' | 'true' | 'false' | INTEGER | FLOAT | STRING
union_pattern  := (literal_pattern | enum_variant) ('|' (literal_pattern | enum_variant))*
enum_variant   := IDENT '.' IDENT
guard          := 'if' expr
arm_body       := expr | block_expr
type_expr      := ... (existing type expression grammar)
```

### Pattern Forms

| Pattern          | Matches                         | Binding                   | Example                                  |
| ---------------- | ------------------------------- | ------------------------- | ---------------------------------------- |
| `name`           | Anything                        | `name` bound to scrutinee | `other => use(other)`                    |
| `_`              | Anything                        | Discarded                 | `_ => "fallback"`                        |
| `name: Type`     | Values of type `T`              | `name` bound (narrowed)   | `s: Success => s.data`                   |
| `_: Type`        | Values of type `T`              | Discarded                 | `_: Failure => "failed"`                 |
| `null`           | `null` value                    | None                      | `null => "nothing"`                      |
| `true` / `false` | Boolean literal                 | None                      | `true => "yes"`                          |
| `42` / `3.14`    | Numeric literal                 | None                      | `200 => "OK"`                            |
| `"foo"`          | String literal                  | None                      | `"start" => "starting"`                  |
| `Enum.Variant`   | Enum variant (value equality)   | None                      | `Status.Active => "active"`              |
| `A \| B`         | Union of literals/enum variants | None                      | `Status.Active \| Status.Pending => ...` |
| `x: A \| B`      | Union of **types** (not values) | `x` bound                 | `x: Success \| Failure => ...`           |

> **Note:** The `Type` after `:` must be an actual type (class, primitive, type alias), not an enum variant. Enum variants are values and must be matched directly without `:` binding.

### Precedence

The `:` in `name: TypeExpr` binds tighter than `|`:

```baml
x: int | bool    // parsed as x: (int | bool)
x: (int | bool)  // same as above
x: (int) | (bool) // same as above
```

## Detailed Examples

### Example 1: Basic Union Discrimination

```baml
class Success { data string, score float }
class Failure { reason string, code int }
type Result = Success | Failure | null

function Process(result: Result) -> string {
  return match (result) {
    null => "No result"
    s: Success => "Got: " + s.data
    f: Failure => "Error: " + f.reason
  }
}
```

### Example 2: Guards for Conditional Matching

Guards add conditions that must be true for the arm to match:

```baml
function Classify(result: Result) -> string {
  return match (result) {
    null => "none"

    s: Success if s.score >= 0.9 => "excellent: " + s.data
    s: Success if s.score >= 0.7 => "good: " + s.data
    s: Success => "marginal: " + s.data

    f: Failure if f.code >= 500 => "server error: " + f.reason
    f: Failure => "client error: " + f.reason
  }
}
```

**Important:** Guards do not contribute to exhaustiveness. A guarded pattern `s: Success if cond` does not cover all `Success` values. You must have an unguarded fallback.

> **Scope note:** This is a simplification for the current proposal. Future versions may support smarter exhaustiveness analysis that recognizes complementary guards (e.g., `if x > 0` and `if x <= 0`) as covering all cases.

### Example 3: Enum Matching

```baml
enum Status { Active, Inactive, Pending, Archived }

function Describe(s: Status) -> string {
  return match (s) {
    Status.Active => "User is active"
    Status.Inactive => "User is inactive"
    Status.Pending => "Awaiting approval"
    Status.Archived => "User archived"
  }
}
```

If you later add `Status.Deleted`, the compiler will error on this match — forcing you to handle the new case.

### Example 4: Literal Unions

```baml
type HttpSuccess = 200 | 201 | 204
type HttpError = 400 | 404 | 500

function DescribeStatus(code: int) -> string {
  return match (code) {
    200 | 201 => "Success with content"
    204 => "Success, no content"
    400 | 404 => "Client error"
    500 => "Server error"
    _ => "Unknown status: " + code
  }
}
```

### Example 5: Variant Unions

```baml
enum Status { Active, Inactive, Pending }

function IsActionable(s: Status) -> bool {
  return match (s) {
    Status.Active | Status.Pending => true
    Status.Inactive => false
  }
}
```

### Example 6: Type Unions with Binding

```baml
type Primitive = string | int | bool
type Complex = User | Image
type Any = Primitive | Complex | null

function Categorize(val: Any) -> string {
  return match (val) {
    null => "nothing"
    p: Primitive => "primitive value"  // p has type string | int | bool
    c: Complex => "complex object"     // c has type User | Image
  }
}
```

### Example 7: Nested Match

```baml
class Request { auth: ApiKey | OAuth | null, endpoint: string }

function Authorize(req: Request) -> string {
  return match (req.auth) {
    null => "No auth for " + req.endpoint

    a: ApiKey => match (a.key) {
      k: string if k.startsWith("prod_") => "Production key"
      _ => "Dev key"
    }

    o: OAuth if o.expires > now() => "Valid OAuth"
    _: OAuth => "Expired OAuth"
  }
}
```

### Example 8: Block Bodies

When an arm needs multiple statements, use a block. The last expression is the result:

```baml
match (status) {
  _: Error => {
    log("Error occurred")
    metrics.increment("errors")
    "Failed"  // return value
  }
  _ => "OK"
}
```

## Exhaustiveness Checking

The compiler performs exhaustiveness analysis to ensure all possible values are handled.

### Rules

1. **All cases must be covered** — either explicitly or via catch-all
2. **Guarded arms don't guarantee coverage** — `s: T if cond` covers only a subset of `T`
3. **Catch-all covers remaining cases** — an untyped binding (`_` or `name`) covers everything not yet matched
4. **Order matters** — first matching arm wins; unreachable arms are a compile error

### Examples

```baml
type T = A | B | C

// OK: all explicit
match (x) {
  _: A => "a"
  _: B => "b"
  _: C => "c"
}

// OK: catch-all covers B and C
match (x) {
  _: A => "a"
  _ => "not a"
}

// ERROR: C not covered
match (x) {
  _: A => "a"
  _: B => "b"
}

// ERROR: unreachable arm (B already covered by catch-all)
match (x) {
  _: A => "a"
  _ => "other"
  _: B => "b"  // unreachable!
}
```

## Semantics

### Evaluation Order

1. Evaluate the scrutinee expression **once**
2. Test arms **top-to-bottom**
3. For each arm:
   - Check if pattern matches
   - If pattern has a guard, evaluate guard
   - If both match, bind variables and evaluate arm body
4. First matching arm's body is the result

### Type Narrowing

Within a matched arm, bound variables have their type narrowed:

```baml
type T = string | int | null

match (x) {
  s: string => s.length()    // s is string, not string | int | null
  n: int => n + 1            // n is int
  null => 0
}
```

### Binding Scope

- Bound variables are in scope in the guard and arm body
- Bound variables do **not** leak outside the arm
- The same binding name can be reused across arms

```baml
match (x) {
  a: A => use(a)   // a is A
  a: B => use(a)   // a is B (different a, shadows previous)
}
// a is not accessible here
```

## Comparison to Other Languages

| Feature            | Rust              | Python 3.10+        | TypeScript | BAML                |
| ------------------ | ----------------- | ------------------- | ---------- | ------------------- |
| **Syntax**         | `match x { ... }` | `match x: case ...` | N/A        | `match (x) { ... }` |
| **Type patterns**  | `Some(x)`         | `case Some(x)`      | N/A        | `x: Type`           |
| **Binding**        | `x @ pattern`     | `case x`            | N/A        | `x: Type` or `x`    |
| **Guards**         | `if cond`         | `if cond`           | N/A        | `if cond`           |
| **Exhaustiveness** | Enforced          | Optional            | N/A        | Enforced            |
| **Literal unions** | `1 \| 2 \| 3`     | `case 1 \| 2 \| 3`  | N/A        | `1 \| 2 \| 3`       |

**Key BAML choices:**
- `:` for type binding (familiar to TS/JS developers)
- Parentheses around scrutinee (familiar to C-family)
- `=>` for arm bodies (familiar to JS arrow functions)
- No special `default` keyword; catch-all is just an untyped binding

## What's NOT in This Proposal

The following features are explicitly **deferred** for future consideration:

### Destructuring (Phase 3 - Future)

```baml
// NOT YET SUPPORTED:
match (user) {
  User { name: "Admin" } => "admin"
  User { name, age } => name + " is " + age
}
```

Destructuring introduces complexity around:
- Nullable field handling (`{ field }` vs `{ field? }`)
- Literal vs variable disambiguation in field patterns
- Nesting depth limits

### Multiple Patterns Per Arm

```baml
// NOT SUPPORTED:
match (x) {
  _: A | _: B => "a or b"  // can't have multiple binding patterns
}

// INSTEAD, use type union:
match (x) {
  _: A | B => "a or b"  // A | B is a type expression
}
```

### `@` Binding (Bind Whole + Destructure)

```baml
// NOT SUPPORTED:
match (x) {
  s @ Success { data } => use(s, data)
}
```

## Implementation Notes

### Parser Changes

Add `parse_match_expr` to the parser, producing:
- `MATCH_EXPR` node containing:
  - Scrutinee expression
  - One or more `MATCH_ARM` nodes

Each `MATCH_ARM` contains:
- Pattern (binding, literal, or union)
- Optional guard expression
- Arm body expression

### Type Checker Changes

1. **Pattern type inference** — determine what type each pattern matches
2. **Exhaustiveness analysis** — compute coverage, error on gaps
3. **Unreachable arm detection** — error on arms that can never match
4. **Type narrowing** — narrow bound variable types within arms

### Codegen Changes

Desugar `match` to equivalent `if-else` chain with `instanceof` checks:

```baml
// Source:
match (x) {
  s: Success if s.score > 0.9 => "high"
  s: Success => "low"
  _: Failure => "failed"
}

// Desugared (conceptually):
{
  let $scrut = x
  if ($scrut instanceof Success) {
    let s = $scrut
    if (s.score > 0.9) {
      "high"
    } else {
      "low"
    }
  } else if ($scrut instanceof Failure) {
    "failed"
  } else {
    // unreachable if exhaustive
  }
}
```

## Key Implementation Caveats

These are subtle points that implementers must handle correctly:

### 1. `_` is NOT a Keyword

Unlike Rust where `_` is a special pattern, in BAML `_` is just an identifier that happens to be dropped. The lexer should emit it as a `WORD` token, not a special token. The "drop" behavior is handled later in the pipeline (name resolution or codegen), not in parsing.

```baml
// These are parsed identically at the syntax level:
_ => "fallback"
other => "fallback"

// The difference is semantic: _ is dropped, other is usable
```

### 2. Type Expressions vs Value Expressions

After `:`, we parse a **type expression**, not a value expression. This means:
- `s: Success` — `Success` is resolved as a type (class name)
- `s: 200 | 201` — `200` and `201` are literal types
- `s: Success | Failure` — union of class types

Without the `:`, we have value patterns:
- `200` — matches the integer value 200
- `Status.Active` — matches the enum variant (via value equality)
- `other` — binds to any value

### 3. Enum Variants Are Values, Not Types

**Important distinction:** Enum variants like `Status.Active` are **values**, not types. You match them directly as value patterns:

```baml
// CORRECT: Enum variant as value pattern (no binding needed)
Status.Active => "active"
Status.Active | Status.Pending => "actionable"

// INCORRECT: Cannot use enum variant after `:` — it's not a type!
// x: Status.Active => ...  // ERROR: Status.Active is a value, not a type
```

To match an enum with binding, match the entire enum type and use guards:

```baml
enum Status { Active, Inactive, Pending }

match (s) {
  // Match specific variants as values (no binding):
  Status.Active => "active"
  Status.Inactive => "inactive"
  Status.Pending => "pending"
}

// Or match the enum type with binding and use guards:
match (s) {
  x: Status if x == Status.Active => "active: " + x
  _: Status => "other status"
}
```

### 4. Guards Don't Affect Exhaustiveness

This is critical for the exhaustiveness checker:

```baml
match (x) {
  s: Success if s.score > 0.9 => "high"
  // ERROR: Success not exhaustively covered!
}
```

Even though `s: Success` appears, the guard makes it partial. The checker must track "guarded coverage" separately from "total coverage."

### 5. Scrutinee Evaluation

The scrutinee must be evaluated exactly once and stored:

```baml
match (expensiveCall()) {
  _: A => ...
  _: B => ...
}

// Must compile to:
let $scrut = expensiveCall()
if ($scrut instanceof A) { ... }
else if ($scrut instanceof B) { ... }

// NOT:
if (expensiveCall() instanceof A) { ... }  // Wrong: multiple calls!
```

### 6. Union Type Precision

When `x: A | B` matches, `x` has type `A | B`, not a supertype:

```baml
class Success { data string }
class Failure { reason string }
class Pending { eta int }

match (result) {
  x: Success | Failure => {
    // x has type: Success | Failure
    // NOT some broader type
  }
  _: Pending => "pending"
}
```

This requires the type checker to construct the exact union type from the pattern.

### 7. First-Match Semantics and Warnings

Order matters. The compiler should error on unreachable arms:

```baml
match (x) {
  _ => "catch all"
  _: Success => "never reached"  // ERROR: unreachable
}
```

But overlapping typed patterns are fine (first wins):

```baml
match (x) {
  _: A => "a"      // matches A
  _: A | B => "ab" // only matches B (A already handled)
}
```

## Open Questions (Resolved)

These questions from earlier drafts have been resolved:

| Question                             | Resolution                                               |
| ------------------------------------ | -------------------------------------------------------- |
| `default` vs `_` for catch-all?      | No special keyword; any untyped binding is catch-all     |
| Guard keyword: `if` or `when`?       | `if` — familiar to all                                   |
| Should `\|` allow multiple patterns? | No; `\|` is always a type/value union within one pattern |
| Require parens in type unions?       | No; precedence is clear (`x: A \| B` = `x: (A \| B)`)    |
