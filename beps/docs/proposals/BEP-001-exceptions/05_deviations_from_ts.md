# Deviations from TypeScript/JavaScript Exceptions

This document lists all semantic and syntactic differences between BAML's Universal Catch and TypeScript/JavaScript exception handling.

## 1. No Variable Binding in Catch Clause

**TypeScript**:
```typescript
try {
  riskyOperation()
} catch (e) {
  console.log(e.message)
}
```
The exception is explicitly bound to `e` in the catch clause header.

**BAML**:
```typescript
riskyOperation() catch {
  e => log(e.message)
}
```
The catch clause contains pattern arms. Variable binding happens within each pattern, not at the clause level.

**Rationale**: The thrown value is implicitly defined by the attached scope—there is no ambiguity about what is being caught. Binding in the header (`catch (e) { ... }`) would require a second level of pattern matching inside. Binding directly in patterns avoids redundancy: `catch { e: TimeoutError => ... }` vs `catch (e) { case e: TimeoutError => ... }`.

## 2. Pattern Matching Syntax

**TypeScript**:
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
Type discrimination requires runtime `instanceof` checks inside the catch body.

**BAML**:
```typescript
riskyOperation() catch {
  e: TimeoutError => retry()
  e: ParseError => null
  _ => throw e
}
```
Type discrimination is part of the pattern syntax. The compiler can perform exhaustiveness analysis.

**Rationale**: Compile-time exhaustiveness checking. Reduced boilerplate.

## 3. No Chained Catch Blocks

**TypeScript (Java-style)**:
```typescript
try {
  riskyOperation()
} catch (e: TimeoutError) {
  retry()
} catch (e: ParseError) {
  return null
}
```
Note: TypeScript does not actually support this syntax. Java does.

**TypeScript (actual)**:
```typescript
try {
  riskyOperation()
} catch (e) {
  // Single catch block with manual type checks
}
```

**BAML**:
```typescript
riskyOperation() catch {
  e: TimeoutError => retry()
  e: ParseError => null
}
```
All patterns are arms within a single catch block.

**Rationale**: First-match semantics are explicit. Exhaustiveness analysis is straightforward because the compiler sees all handlers at once.

**Note**: Developers can achieve chained catch behavior by nesting catch blocks or using explicit `throw` statements, since unmatched patterns in a catch block implicitly re-throw. The idiomatic approach is pattern matching within a single catch block.

## 4. Expression Semantics

**TypeScript**:
```typescript
// try/catch is a statement
let result
try {
  result = riskyOperation()
} catch (e) {
  result = null
}
```
Variables must be hoisted outside the try block to be accessible after.

**BAML**:
```typescript
let result = riskyOperation() catch { _ => null }
```
The `catch` expression evaluates to the union of the try expression's type and each handler's return type.

**Rationale**: Expression-oriented syntax reduces variable hoisting and supports inline fallbacks.

## 5. Try Keyword is Optional

**TypeScript**:
```typescript
try {
  riskyOperation()
} catch (e) {
  handleError(e)
}
```
The `try` keyword is required.

**BAML**:
```typescript
// Explicit try (optional)
try {
  riskyOperation()
} catch {
  e => handleError(e)
}

// Implicit try (semantically identical)
{
  riskyOperation()
} catch {
  e => handleError(e)
}
```
The `try` keyword is syntactic sugar. Any block can have a catch attached.

**Rationale**: `try` signals intent but adds no semantic meaning. Omitting it reduces noise in common patterns.

## 6. Catch Attaches to Function Bodies

**TypeScript**:
```typescript
function extract(text: string): Resume | null {
  try {
    // function body
    return callLLM(text)
  } catch (e) {
    return null
  }
}
```
The function body must be wrapped in a try block. Declarative constructs cannot be directly protected.

**BAML**:
```typescript
function Extract(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"Extract resume from {{ text }}"#
} catch {
  _ => null
}
```
The catch block attaches directly to the function, treating the entire body as the try scope.

**Rationale**: Additive error handling without restructuring the function body. Preserves declarative syntax for LLM functions.

## 7. Catch Attaches to Control Flow Statements

**TypeScript**:
```typescript
for (const item of items) {
  try {
    process(item)
  } catch (e) {
    console.log(`Failed: ${item}`)
  }
}
```
Error handling requires an inner try/catch block.

**BAML**:
```typescript
for (item in items) {
  process(item)
} catch {
  e => log(`Failed: ${item}`)
}
```
Catch attaches to the loop statement. Errors are handled per-iteration. Execution continues to the next iteration.

**Rationale**: Common batch processing pattern without extra nesting.

## 8. Inline Catch on Expressions

**TypeScript**:
```typescript
// IIFE pattern
const result = (() => {
  try {
    return riskyOperation()
  } catch (e) {
    return null
  }
})()
```
Inline error handling requires an immediately-invoked function expression.

**BAML**:
```typescript
let result = riskyOperation() catch { _ => null }
```
Catch is a postfix operator on expressions.

**Rationale**: Concise inline fallbacks without wrapping in IIFE.

## 9. No Finally Clause

**TypeScript**:
```typescript
let handle
try {
  handle = acquireResource()
  useResource(handle)
} catch (e) {
  logError(e)
} finally {
  if (handle) {
    releaseResource(handle)
  }
}
```
The `finally` block runs regardless of success or failure.

**BAML**:
```typescript
let handle = acquireResource()
let result = try {
  useResource(handle)
} catch {
  e => {
    logError(e)
    null
  }
}
releaseResource(handle)
```
Cleanup is handled through normal control flow. No `finally` keyword.

**Rationale**: Simpler model. May revisit if common patterns emerge that require `finally`.

## 10. Pattern Arms Use Arrow Syntax

**TypeScript**:
```typescript
try {
  riskyOperation()
} catch (e) {
  // Block body with statements
  console.log(e)
  return null
}
```
The catch clause uses block syntax.

**BAML**:
```typescript
riskyOperation() catch {
  e => {
    log(e)
    null
  }
}
```
Each pattern arm uses `=>` arrow syntax. Multi-statement handlers use blocks.

**Rationale**: Consistency with `match` expressions.

## 11. Wildcard Does Not Catch Panics

**TypeScript**:
```typescript
try {
  riskyOperation()
} catch (e) {
  // Catches everything, including bugs
}
```
All thrown values are caught.

**BAML**:
```typescript
riskyOperation() catch {
  _ => null           // Catches recoverable errors
  // IndexOutOfBounds, AssertionError, etc. propagate
}

// To catch panics explicitly:
riskyOperation() catch {
  p: Panic => handleBug(p)
  _ => null
}
```
The `_` wildcard matches recoverable errors but not `Panic` types. Panics must be caught explicitly.

**Rationale**: Bugs should fail loudly by default. Silent swallowing of logic errors masks problems.

## 12. Thrown Values Are Not Restricted to Error Types

**TypeScript**:
```typescript
throw "string error"     // Valid but discouraged
throw new Error("msg")   // Idiomatic
throw { code: 500 }      // Valid but discouraged
```
Convention is to throw `Error` instances, but any value can be thrown.

**BAML**:
```typescript
throw TimeoutError("operation timed out")
throw { code: 500, message: "server error" }
```
BAML uses an open throw system. Any value can be thrown. There is no required base `Error` type.

**Rationale**: Flexibility for domain-specific error types without requiring inheritance.

## Summary Table

| Aspect | TypeScript | BAML |
|:-------|:-----------|:-----|
| Variable binding | In clause header: `catch (e)` | In pattern: `catch { e => ... }` |
| Type discrimination | Runtime `instanceof` | Pattern matching |
| Multiple handlers | Manual `if/else` | Pattern arms in one block |
| Expression vs statement | Statement (needs IIFE for expression) | Expression |
| Try keyword | Required | Optional |
| Attach to functions | No | Yes |
| Attach to loops | No | Yes |
| Inline catch | Requires IIFE | Postfix operator |
| Finally | Supported | Not supported |
| Arrow syntax | No | Yes |
| Wildcard catches all | Yes | No (excludes Panic) |
| Thrown value types | Any (convention: Error) | Any (no convention) |

