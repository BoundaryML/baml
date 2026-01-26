# Design: Why Inline `.catch()` Was Removed

## The Problem

The original proposal included an **inline catch** syntax:

```baml
let x = Foo().catch({
   MyError() => { fallback() }
})
```

This syntax creates **semantic confusion** about the nature of errors in BAML.

## The Core Issue: Errors as Values vs. Runtime Events

The `.catch()` syntax implies that errors are **values** that can be operated on with the `.` (record access) operator. This creates a conceptual problem:

```baml
function A() -> int {
  let x = SomethingThatThrows();  // Is x an error value here?
  let y = x.catch({});             // If so, why can we call .catch on it?
  y
}
```

In reality, errors in BAML are **runtime events** that occur during statement execution, not values that get assigned to variables.

### The Semantic Confusion

Consider this progression:
1. `let x = Foo();` is a **statement**.
2. `Foo()` is an **expression**.
3. Evaluating the expression might **fail** (throw an error).
4. A failing expression is **not** an error value.
5. Yet `Foo().catch({})` implies that `.catch` is being called on some value.

The `.` operator binds more tightly than whitespace, meaning `catch` appears to be a method/field on the result of `Foo()`. This creates the illusion that errors are values.

### Complex Semantic Rules Required

To make `.catch()` work correctly, we'd need complex rules like:

> "The meaning of `.catch()` when applied to a function means that the function changes into a form that will run the catch handler if evaluating that function fails."

Rules like this tend to have bad interactions with other language features and make the semantics harder to understand.

## Explored Alternatives

### Alternative 1: Whitespace-Based Syntax (Rejected)

```baml
let x = Thrower() catch { ... };  // catch is part of the statement, not the expression
```

This avoids the `.` operator problem but introduces its own confusion about operator precedence and statement/expression boundaries.

### Alternative 2: Expression Blocks (Adopted)

The simplest and most consistent solution is to use **expression blocks**, which are already part of BAML's syntax:

```baml
function A() -> int {
  let x = {
     catch { ... }
     Thrower()
  };
}
```

**Why this works:**

- `catch` is clearly the first statement in its scope (the block).
- No confusion about whether errors are values.
- Consistent with the scope-level `catch` semantics.
- No new syntax required.

## Decision

We removed `.catch()` from the proposal and recommend using expression blocks for call-site error handling:

```baml
// Instead of:
let x = Foo().catch({ MyError() => { fallback() } })

// Use:
let x = {
   catch { MyError() => { fallback() } }
   Foo()
}
```

This maintains semantic clarity while still allowing localized error handling when needed.
