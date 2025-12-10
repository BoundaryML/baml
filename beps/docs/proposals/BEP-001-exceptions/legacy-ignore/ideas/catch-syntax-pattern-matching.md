# Catch Block Syntax: Pattern Matching vs. Multi-Catch

**Date**: 2025-12-03

## Overview

BAML uses pattern matching syntax inside `catch` blocks:

```typescript
} catch {
  e: TimeoutError => retry()
  e: ParseError => null
  _ => defaultValue()
}
```

This differs from the multi-catch syntax common in languages like Java, C#, and JavaScript:

```typescript
// Multi-catch syntax (not used in BAML)
} catch (e: TimeoutError) {
  retry()
} catch (e: ParseError) {
  return null
} catch (e) {
  return defaultValue()
}
```

This document examines the rationale and trade-offs.

## Pattern Matching Syntax

### Consistency with Match Expressions

BAML's `match` expression uses pattern matching:

```typescript
let result = match (value) {
  User { role: "admin" } => handleAdmin()
  User { role: "user" } => handleUser()
  _ => handleGuest()
}
```

Using the same syntax for `catch` blocks reduces the number of distinct syntactic forms developers need to learn.

### Exhaustiveness Checking

Pattern matching provides a natural foundation for exhaustiveness checking:

```typescript
} catch {
  e: TimeoutError => retry()
  e: ParseError => null
  // Compiler can determine if all Error types are covered
}
```

This aligns with the `safe` keyword's requirement that all `Error` types be handled.

### Destructuring

Pattern matching supports destructuring error objects:

```typescript
} catch {
  ApiError { code: 404, message } => handleNotFound(message)
  ApiError { code: 500 } => handleServerError()
  e: NetworkError => logAndRetry(e)
  _ => defaultValue()
}
```

### Single Block Structure

All error handlers are contained in one syntactic block:

```typescript
} catch {
  e: TimeoutError => retry()
  e: ParseError => null
  e: NetworkError => fallback()
  _ => defaultValue()
}
```

### Implicit Desugaring

The pattern matching structure accommodates implicit pattern insertion for the Panic/Error distinction:

```typescript
// Source:
} catch {
  e: TimeoutError => retry()
  _ => defaultValue()
}

// Desugared:
} catch {
  e: TimeoutError => retry()
  _: Error => defaultValue()
  _p: Panic => throw _p  // implicit
}
```

### Expression Context

Pattern matching syntax works naturally in expression contexts:

```typescript
let result = GetData() catch {
  e: TimeoutError => retry()
  _ => null
}
```

## Multi-Catch Alternative

### Syntax

```typescript
} catch (e: TimeoutError) {
  retry()
} catch (e: ParseError) {
  return null
} catch (e) {
  return defaultValue()
}
```

### Trade-offs

**Advantages:**
- Familiar to developers from Java, C#, JavaScript
- Each handler is visually distinct
- No new syntax to learn

**Disadvantages:**
- Requires repeating the `catch` keyword
- Inconsistent with BAML's `match` expression syntax
- Exhaustiveness checking is less straightforward (no clear delimiter for the end of the catch chain)
- Destructuring would require additional syntax
- Awkward in expression contexts
- Unclear where to insert implicit patterns for Panic/Error distinction

### Expression Context Example

```typescript
// Pattern matching:
let result = GetData() catch {
  e: TimeoutError => retry()
  _ => null
}

// Multi-catch (unclear how this would work):
let result = GetData() 
  catch (e: TimeoutError) { retry() }
  catch (e) { return null }
```

## Other Alternatives Considered

### Hybrid Approach

Allow both syntaxes:

```typescript
// Pattern matching
} catch {
  e: TimeoutError => retry()
  _ => null
}

// Multi-catch
} catch (e: TimeoutError) {
  retry()
} catch (e) {
  return null
}
```

**Trade-off:** Provides flexibility but introduces two ways to accomplish the same task, leading to inconsistent codebases and increased tooling complexity.

### Explicit Match Keyword

```typescript
} catch match {
  e: TimeoutError => retry()
  _ => null
}
```

**Trade-off:** More explicit but adds verbosity. The `{ ... }` syntax already signals pattern matching.

### Block Syntax Instead of Arrows

```typescript
} catch {
  e: TimeoutError {
    retry()
  }
  _ {
    return defaultValue()
  }
}
```

**Trade-off:** More similar to traditional `switch` statements but inconsistent with BAML's `match` expression, which uses arrow syntax.

## Selected Approach

BAML uses pattern matching syntax:

```typescript
} catch {
  pattern1 => handler1
  pattern2 => handler2
  _ => defaultHandler
}
```

This provides:
- Consistency with `match` expressions
- Natural exhaustiveness checking
- Destructuring support
- Compact syntax
- Clear expression semantics
- Straightforward desugaring for implicit patterns

## Critical Analysis

### Unfamiliarity for Most Developers

The pattern matching syntax in `catch` blocks is unfamiliar to developers coming from mainstream languages:

- **JavaScript/TypeScript**: Single `catch (e)` with manual type checking
- **Java/C#**: Multi-catch with `catch (Type e)` syntax
- **Python**: `except Type as e:` syntax
- **Go**: No exceptions, error values

Only Scala uses pattern matching in catch blocks. This means most developers will need to learn a new syntax for error handling, even if they're familiar with exceptions in other languages.

**Question**: Does the consistency with BAML's `match` expression outweigh the unfamiliarity for developers who don't know Scala?

### Cognitive Load: Two Concepts in One

Pattern matching in `catch` blocks combines two distinct concepts:

1. **Exception handling**: Which errors to catch
2. **Pattern matching**: How to destructure and match values

```typescript
} catch {
  ApiError { code: 404, message } => handleNotFound(message)
  ApiError { code: 500 } => handleServerError()
  e: NetworkError => logAndRetry(e)
  _ => defaultValue()
}
```

Developers must understand:
- Exception propagation semantics
- Pattern matching semantics
- Destructuring syntax
- Exhaustiveness checking
- The implicit Panic/Error distinction

**Question**: Is this cognitive overhead justified, or would a simpler syntax (even if more verbose) be easier to reason about?

### Error Messages and Debugging

When exhaustiveness checking fails, error messages must explain both pattern matching and exception handling:

```typescript
} catch {
  e: TimeoutError => retry()
  // Error: Non-exhaustive catch block
  // Missing patterns for: ParseError, NetworkError
  // Or add wildcard pattern: _ => ...
}
```

Developers need to understand:
- What "exhaustive" means in the context of exceptions
- How to add patterns to cover missing cases
- The difference between `_` and `e` as wildcards
- Why `_: Error` is different from `_: Panic`

**Question**: Will error messages be clear enough for developers unfamiliar with pattern matching?

### Ordering Semantics

Pattern matching has first-match semantics, which can be surprising:

```typescript
} catch {
  _ => defaultValue()           // Catches everything
  e: TimeoutError => retry()    // Never reached!
}
```

This is different from some multi-catch implementations where specificity matters. Developers must understand that order matters.

**Question**: Will developers expect specificity-based matching instead of first-match?

### Interaction with Control Flow

Pattern matching syntax can make certain control flow patterns less obvious:

```typescript
} catch {
  e: TimeoutError => {
    if (retryCount < 3) {
      return retry()
    } else {
      return null
    }
  }
  _ => null
}
```

Multi-statement handlers require block syntax, which can become verbose. The arrow syntax suggests expression-oriented code, but handlers often need multiple statements.

**Question**: Does the arrow syntax create false expectations about handler complexity?

### Trailing Catch Ambiguity

With trailing catch syntax, it's not immediately clear what scope the catch covers:

```typescript
function Process() -> Result {
  let x = GetData()
  let y = Transform(x)
  return y
} catch {
  _ => null
}
```

Does the catch cover:
- Just the `return y` statement?
- The entire function body?
- Something else?

(Answer: the entire function body, but this may not be obvious)

**Question**: Does the trailing position make the scope clear enough, or should there be a more explicit marker?

### Destructuring Complexity

While destructuring is powerful, it can make catch blocks harder to read:

```typescript
} catch {
  ApiError { code: 404, message, details: { retryAfter } } => {
    log.warn(message)
    scheduleRetry(retryAfter)
    return null
  }
  ApiError { code, message } => handleError(code, message)
  _ => defaultValue()
}
```

**Question**: Does the expressiveness of destructuring justify the added complexity in error handling code?

### Learning Curve

Developers must learn:
1. Pattern matching syntax (if not already familiar)
2. How pattern matching applies to exceptions
3. Exhaustiveness checking rules
4. The Panic/Error distinction
5. Implicit pattern desugaring
6. Ordering semantics

This is a steeper learning curve than traditional multi-catch.

**Question**: Is the learning investment worth the benefits, especially for developers who only occasionally write BAML code?

### Alternative: Could Multi-Catch Be Extended?

Multi-catch syntax could potentially support the same features with extensions:

```typescript
// Hypothetical: multi-catch with destructuring
} catch (ApiError { code: 404, message }) {
  handleNotFound(message)
} catch (e: TimeoutError) {
  retry()
} catch (e) {
  defaultValue()
}
```

**Question**: Could we achieve the same goals (destructuring, exhaustiveness) with a more familiar syntax?

## Language Comparisons

### Scala

Scala uses pattern matching in `catch`:

```scala
try {
  riskyOperation()
} catch {
  case e: TimeoutError => retry()
  case e: ParseError => null
  case _ => defaultValue()
}
```

BAML's syntax is similar but uses `=>` instead of `case`.

### Swift

Swift uses multi-catch:

```swift
do {
    try riskyOperation()
} catch let error as TimeoutError {
    retry()
} catch {
    return defaultValue()
}
```

### Rust

Rust doesn't have exceptions but uses `match` for `Result` types:

```rust
match result {
    Ok(value) => handle_value(value),
    Err(e) => handle_error(e),
}
```

### TypeScript/JavaScript

Single `catch` with manual type checking:

```typescript
try {
  riskyOperation()
} catch (e) {
  if (e instanceof TimeoutError) {
    retry()
  } else {
    return defaultValue()
  }
}
```

## Conclusion: Does Pattern Matching Justify the Deviation?

### The Burden of Proof

BAML's design philosophy is to follow TypeScript conventions unless there is a **substantial benefit** to users that justifies the learning cost of a different syntax.

TypeScript's exception handling is:
```typescript
try {
  riskyOperation()
} catch (e) {
  if (e instanceof TimeoutError) {
    retry()
  } else {
    return null
  }
}
```

This is familiar, well-understood, and requires no new syntax to learn.

**The question**: Does pattern matching in `catch` blocks provide enough value to justify deviating from this familiar pattern?

### Arguments For Pattern Matching

#### 1. Exhaustiveness Checking is Critical for AI Code

In AI engineering, error handling is not exceptional—it's routine. LLMs fail frequently and predictably. The `safe` keyword requires exhaustive error handling.

Pattern matching provides **compile-time guarantees** about exhaustiveness:
```typescript
// Compiler error: missing NetworkError
} catch {
  e: TimeoutError => retry()
  e: ParseError => null
}
```

TypeScript's `try/catch` cannot provide this guarantee. Developers must manually ensure all error types are handled.

**Value**: Prevents bugs from unhandled error types at compile time, not runtime.

#### 2. Trailing Catch Requires Expression Semantics

BAML's trailing catch syntax is designed to be additive—you append error handling without restructuring code:

```typescript
function Extract() -> Resume | null {
  client "gpt-4o"
  prompt #"..."#
} catch {
  _ => null
}
```

This is fundamentally **expression-oriented**. The catch block is part of the function's value, not a separate statement.

Multi-catch syntax doesn't naturally support this:
```typescript
// How would this work?
function Extract() -> Resume | null {
  client "gpt-4o"
  prompt #"..."#
} catch (e: TimeoutError) {
  return null
} catch (e) {
  return null
}
```

The `return` statements are awkward—they're inside the catch clauses but conceptually part of the function's return value.

**Value**: Pattern matching syntax aligns with expression-oriented error handling.

#### 3. Inline Catch Requires Compact Syntax

Inline catch is a core feature:
```typescript
let user = GetUser(id) catch { _ => null }
```

Multi-catch would be verbose:
```typescript
let user = GetUser(id) catch (e) { return null }
```

And with multiple handlers:
```typescript
// Pattern matching:
let data = Fetch() catch {
  e: Timeout => retry()
  _ => null
}

// Multi-catch:
let data = Fetch() catch (e: Timeout) { return retry() } catch (e) { return null }
```

**Value**: Conciseness matters for inline error handling.

#### 4. Implicit Desugaring for Panic/Error

The Panic/Error distinction requires implicit pattern insertion:
```typescript
} catch {
  e: TimeoutError => retry()
  _ => null
}
// Desugars to insert: _p: Panic => throw _p
```

With multi-catch, where does the implicit handler go?
```typescript
} catch (e: TimeoutError) {
  retry()
} catch (e) {
  return null
}
// Insert implicit panic handler... where?
```

Pattern matching provides a clear insertion point (after user patterns, before the implicit panic re-throw).

**Value**: Clean semantics for the Panic/Error distinction.

#### 5. Consistency with Match

BAML already has `match` expressions with pattern matching. Adding pattern matching to `catch` means learning **one** pattern matching syntax that works in two places, not two different syntaxes.

**Value**: Reduced cognitive load overall (one pattern matching syntax vs. two different error handling syntaxes).

### Arguments Against Pattern Matching

#### 1. Unfamiliarity

Most developers don't know Scala. They will need to learn pattern matching syntax specifically for error handling.

**Counter**: BAML already requires learning pattern matching for `match` expressions. The marginal cost is lower than it appears.

#### 2. Cognitive Complexity

Combining exception handling with pattern matching increases cognitive load.

**Counter**: The alternative (TypeScript-style manual type checking) also has cognitive load—it's just different. Pattern matching makes the compiler do the work.

#### 3. Steeper Learning Curve

Pattern matching has more concepts to learn than simple `try/catch`.

**Counter**: The learning investment pays off through compile-time safety and more expressive error handling.

### The Verdict

Pattern matching in `catch` blocks **does** justify the deviation from TypeScript syntax, but only because of the **combination** of factors:

1. **Exhaustiveness checking** is essential for `safe` functions
2. **Trailing catch** requires expression semantics
3. **Inline catch** requires compact syntax
4. **Panic/Error distinction** requires implicit desugaring
5. **Match expressions** already exist in BAML

No single factor alone would justify the deviation. But together, they create a **compounding benefit** that outweighs the learning cost.

### What Would Make Us Reconsider?

**Important caveat**: Pattern matching syntax can desugar to chained catch clauses at the implementation level. This means most of the technical capabilities (exhaustiveness checking, Panic/Error distinction, etc.) could theoretically be achieved with either syntax.

The real question is: **Which surface syntax provides better developer ergonomics?**

We would reconsider pattern matching if:

1. **Developer feedback** shows that the pattern matching syntax is significantly harder to learn or use than anticipated
2. **Error messages** cannot be made clear enough for developers unfamiliar with pattern matching
3. **Tooling support** (IDE autocomplete, refactoring) is significantly harder to implement for pattern matching
4. **Match expressions are removed** from BAML (eliminating the consistency argument)
5. **Inline catch becomes less important** in practice, reducing the value of compact syntax

The technical capabilities are achievable with either syntax through desugaring. The choice is about which syntax better serves developers writing and reading BAML code.

### Recommendation

Use pattern matching syntax in `catch` blocks. The deviation from TypeScript is justified by the combination of:
- Compile-time exhaustiveness guarantees
- Expression-oriented semantics for trailing/inline catch
- Clean Panic/Error distinction
- Consistency with existing `match` expressions

The learning cost is real, but the benefits compound in ways that TypeScript-style syntax cannot match.
