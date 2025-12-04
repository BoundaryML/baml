# Design Alternatives

This document outlines the alternative error handling designs considered for BAML and why they were rejected.

## Methodology

We evaluated designs against two scenarios:

1.  **Declarative**: LLM functions defined with `client` and `prompt` blocks (configuration-heavy).
2.  **Imperative**: Functions with control flow and variable assignments (logic-heavy).

The goal was a unified syntax that works ergonomically in both contexts without forcing structural refactoring.

## Rejected Designs

### 1. Result Types (`Result<T, E>`)

Treating errors as values, similar to Rust or Go.

```typescript
function Extract(text: string) -> Result<Resume, Error> { ... }

let result = Extract(text)
match result {
  Ok(resume) => ...
  Err(e) => ...
}
```

**Why it was rejected:**

*   **Viral Complexity**: Changing a return type to `Result` forces all callers to update their signatures to handle or propagate it.
*   **Verbosity**: Unwrapping results or mapping errors adds significant overhead for scripting and prototyping.
*   **Type System Overhead**: Requires a complex sum type system and dedicated error interfaces, increasing the learning curve for users from dynamic languages.

### 2. Classic `try/catch` Statement

The traditional block-based structure found in Java and TypeScript.

```typescript
// Imperative
function ProcessBatch(urls: string[]) -> Resume[] {
  // 1. Hoisting Tax: Declare variable with nullable type
  let aggregator: MetricsAggregator | null = null
  
  // 2. Indentation Tax: Wrap initialization
  try {
    aggregator = MetricsAggregator.new()
  } catch {
    log.warn("Failed to initialize aggregator")
  }
  
  let results = []
  
  for (url in urls) {
    let resume = ExtractResume(url)
    
    // 3. Safety Tax: Check for null on every use
    if (aggregator != null) {
      aggregator.record(resume)
    }
    results.append(resume)
  }
  return results
}

// Declarative
function Extract(text: string) -> Resume | null {
  try {
    // ❌ Confusing: Are we "trying" to define the client?
    client "gpt-4o"
    prompt #"Extract resume from {{ text }}"#
  } catch {
    _: TimeoutError => null
  }
}
```

**Why it was rejected:**

*   **Indentation Tax**: Adding error handling requires indenting the entire block, creating large diffs for a wrapper concept.
*   **Hoisting Tax**: Variables declared inside `try` are not accessible in `catch` or after the block, forcing declarations to be moved ("hoisted") outside.
*   **Declarative Incompatibility**: `try` implies sequential execution. Wrapping declarative properties like `client` and `prompt` in an imperative block creates a semantic mismatch and syntax errors.

### 3. Expression-Oriented Try (`let x = try { ... }`)

Allowing `try` blocks to return values, like in Kotlin.

```typescript
let resume = try {
  Extract(text)
} catch {
  _ => null
}
```

**Why it was rejected:**

*   **Partial Solution**: Solves the variable hoisting issue but fails in declarative contexts. `client` declarations cannot be wrapped in an expression.
*   **Syntactic Noise**: Frequent use of the `try` keyword becomes repetitive.
*   **Declarative Confusion**: Suggests that configuration blocks are expressions, which misrepresents the execution model.

### 4. Function Modifiers

Using a keyword in the signature to denote error handling.

```typescript
function Extract(text: string) -> Resume try {
  client "gpt-4"
  prompt #"..."#
} catch {
  _ => null
}
```

**Why it was rejected:**

*   **Syntactic Irregularity**: Introduces a special grammar rule that doesn't compose with other constructs.
*   **Limited Scope**: Solves function boundaries but offers no solution for statement-level (loops) or expression-level handling.

### 5. Wrapper Functions

Encouraging separate "safe" wrapper functions.

```typescript
function Extract(text: string) -> Resume | null {
  try {
    return _ExtractInternal(text)
  } catch {
    return null
  }
}
```

**Why it was rejected:**

*   **Boilerplate**: Doubles the function count for simple error handling.
*   **Tooling Degradation**: Breaks the link between the prompt definition and the execution context (e.g., "Prompt Preview" or "Run Function" features).
*   **Cognitive Load**: Developers must manage and recall two versions of every function.

### 6. Checked Exceptions (`throws` clause)

Requiring functions to explicitly declare the errors they throw in their signature, similar to Java.

```typescript
function Extract(text: string) -> Resume throws TimeoutError, ParseError { ... }
```

**Why it was rejected:**

*   **Virality**: Like `Result` types, adding a new error type to a low-level function forces updates to every caller in the stack to either handle or propagate it.
*   **Boilerplate**: Often leads to developers declaring generic `throws Error` to avoid maintenance burden, defeating the purpose of strict checking.
*   **Prototyping Friction**: Slows down the "prototype to production" loop by demanding rigorous error specifications upfront, rather than allowing them to emerge during hardening.

### 7. Chained Catch Blocks

Using multiple `catch` blocks for different error types, standard in languages like Java or JavaScript.

```typescript
try {
    Extract(text) 
} catch (e: TimeoutError) {
    retry()
} catch (e: ParseError) {
    return null
}
```

**Why it was rejected:**

*   **Exhaustiveness Ambiguity**: Unlike pattern matching where the compiler sees the full set of handlers at once, chained blocks are evaluated sequentially. It's harder for the compiler (and the user) to reason about whether the *union* of all catch blocks covers every possible error type, especially when mixing specific types and generic wildcards.
    * e.g. where do you put the error for exhaustiveness missing? on the try? the last catch?
*   **Verbosity**: Requires repeating the `catch` keyword and block overhead for every error type, making it too heavy for inline expressions.
*   **Expression Semantics**: A single `catch` block using pattern matching (`catch { case1 => ...; case2 => ... }`) composes better as an expression than a chain of statement blocks.
