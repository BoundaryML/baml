# Coming from TypeScript/JavaScript

This document answers common questions from developers familiar with TypeScript/JavaScript exception handling.

---

### Why does catch use pattern matching instead of `catch (e) { ... }`?

**TypeScript** binds one variable, then uses `instanceof` to discriminate:

```typescript
try {
  riskyOperation()
} catch (e) {
  if (e instanceof TimeoutError) {
    retry()
  } else if (e instanceof ParseError) {
    return null
  } else {
    throw e
  }
}
```

**BAML** uses pattern matching with arrow syntax:

```typescript
riskyOperation() catch {
  e: TimeoutError => retry()
  e: ParseError => null
  // Other errors implicitly propagate
}
```

**Why not `catch (e) { <pattern matching> }`?**

We could have kept the header binding and added pattern matching inside:

```typescript
// Hypothetical: header binding + pattern matching
catch (e) {
  e: TimeoutError => retry()
  e: ParseError => null
}
```

But this creates redundancy: you bind `e` in the header, then re-bind it in each pattern arm. Which `e` is in scope? The outer untyped one or the inner typed one? 

By removing the header binding, the design is cleaner: each pattern arm introduces its own binding with the matched type already applied.

**Other benefits:**

1. **Untyped patterns exclude Panics**: A pattern like `e` matches all Errors but not Panics. Bugs crash loudly by default. To catch panics explicitly: `p: Panic => ...`.

2. **Implicit re-throw**: Unhandled cases propagate automatically. No `else { throw e }` boilerplate.

3. **Arrow syntax**: Consistent with `match` expressions. Single expressions don't need braces (`e => null`), multi-statement handlers use blocks.

See [Design Alternatives](./03_alternatives.md#why-pattern-matching-syntax-in-catch) for the full rationale.

---

### Do I need to write `try` before every catch?

**TypeScript** requires `try`:

```typescript
try {
  riskyOperation()
} catch (e) {
  handleError(e)
}
```

**BAML** allows `catch` on any expression, so `try` is optional:

```typescript
// Catch on a function call
a() catch { e => null }

// Catch on a binary expression
a() + b() catch { e => 0 }

// Catch on a block expression
{ let x = a(); x + b() } catch { e => 0 }

// Catch on a block with explicit try (for familiarity)
try { let x = a(); x + b() } catch { e => 0 }
```

Since `catch` attaches to any expression—including block expressions—the `try` keyword is redundant. We allow `try` as a prefix for familiarity, but it adds no semantic meaning.

### How do I add error handling to a function without wrapping the body?

**TypeScript** requires wrapping the function body:

```typescript
function extract(text: string): Resume | null {
  try {
    return callLLM(text)
  } catch (e) {
    return null
  }
}
```

**BAML** attaches catch directly to the function:

```typescript
function Extract(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"Extract resume from {{ text }}"#
} catch {
  e => null
}
```

No restructuring needed. Particularly useful for declarative LLM functions.

### How do I handle errors in a loop without nesting try/catch?

**TypeScript** requires an inner try/catch:

```typescript
for (const item of items) {
  try {
    process(item)
  } catch (e) {
    console.log(`Failed: ${item}`)
  }
}
```

**BAML** attaches catch to the loop:

```typescript
for (item in items) {
  process(item)
} catch {
  e => log(`Failed: ${item}`)
}
```

Errors are handled per-iteration. Execution continues to the next item.

---

### Why is `catch` an expression instead of a statement?

**TypeScript** try/catch is a statement, requiring variable hoisting or an IIFE:

```typescript
// Hoisting required
let result
try {
  result = riskyOperation()
} catch (e) {
  result = null
}

// Or use an IIFE
const result2 = (() => {
  try { return riskyOperation() }
  catch (e) { return null }
})()
```

**BAML** catch is an expression—no hoisting or wrapping needed:

```typescript
let result = riskyOperation() catch { e => null }
```

The result type is the union of the success type and handler return types.

### Why doesn't my catch-all pattern catch `IndexOutOfBounds`?

**TypeScript** catches everything:

```typescript
try {
  riskyOperation()
} catch (e) {
  // Catches everything, including bugs
}
```

**BAML** distinguishes errors from panics:

```typescript
riskyOperation() catch {
  e => null  // Catches recoverable errors only
  // IndexOutOfBounds, AssertionError, etc. propagate
}

// To catch panics explicitly:
riskyOperation() catch {
  p: Panic => handleBug(p)
  e => null
}
```

Untyped patterns like `e` match recoverable errors but not `Panic` types. Bugs fail loudly by default.

### Is there a `finally` block?

**TypeScript** supports `finally`:

```typescript
let handle
try {
  handle = acquireResource()
  useResource(handle)
} catch (e) {
  logError(e)
} finally {
  if (handle) releaseResource(handle)
}
```

**BAML** handles cleanup through normal control flow:

```typescript
let handle = acquireResource()
let result = {
  useResource(handle)
} catch {
  e => {
    logError(e)
    null
  }
}
releaseResource(handle)
```

No `finally` keyword. Place cleanup after the catch expression.

### What types can I throw?

**TypeScript** allows any value but convention is `Error`:

```typescript
throw "string error"     // Valid but discouraged
throw new Error("msg")   // Idiomatic
throw { code: 500 }      // Valid but discouraged
```

**BAML** uses an open throw system:

```typescript
throw TimeoutError("operation timed out")
throw { code: 500, message: "server error" }
```

Any value can be thrown. No required base `Error` type.

---

## Summary Table

| Question | TypeScript | BAML |
|:---------|:-----------|:-----|
| How do I handle errors by type? | `catch (e) { if (e instanceof ...) }` | Pattern matching: `catch { e: Type => ... }` |
| Can I use catch as an expression? | No (needs IIFE) | Yes |
| Is `try` required? | Yes | No (optional) |
| Can I attach catch to functions? | No | Yes |
| Can I attach catch to loops? | No | Yes |
| Is there a `finally`? | Yes | No |
| Does catch-all catch everything? | Yes | No (excludes Panic) |
| What can I throw? | Any (convention: Error) | Any (no convention) |
