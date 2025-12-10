# Proposal: Universal Catch

This document outlines the proposed error handling syntax for BAML.

## Core Concept

The core proposal is **Universal Catch**: `catch` is an operator that attaches to *any* block or expression, not just a `try` statement.

In BAML, any scope that executes code (a function body, a loop body, a block expression) implicitly acts as a boundary for error propagation. The `catch` clause attaches to that boundary to handle errors that propagate out of it.

## The Three Forms

Universal Catch supports three syntactic forms across declarative and imperative contexts.

### 1. Function-Level Catch

For function declarations, `catch` attaches directly to the function body. This treats the entire function execution as an implicit try block.

```typescript
function ExtractResume(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"Extract resume from {{ text }}"#
} catch {
  e: TimeoutError => null
  e: RefusalError => null
}
```

This form allows adding error handling to declarative LLM functions without wrapping the configuration in an imperative `try` block or changing the indentation.

### 2. Expression Catch

In BAML, blocks `{ ... }` are expressions. This means `catch` can be attached to any expression, whether it's a simple function call or a complex block.

#### Inline Catch

For error handling on single expressions, `catch` acts as a postfix operator.

```typescript
let user = GetUser(id) catch { e => null }
```

The operator precedence of `catch` is lower than other operators, meaning it applies to the entire preceding expression unless grouped by parentheses.

#### Block Expressions

Since blocks are expressions, `catch` attaches naturally to them:

```typescript
let aggregator = {
  MetricsAggregator.new()
} catch {
  e => {
    log.warn("Failed to initialize aggregator", e)
    null
  }
}
```

**Optional `try` Keyword**:
The `try` keyword is valid in BAML but functions purely as optional syntactic sugar for an imperative block expression.

*   `{ ... } catch { ... }` is semantically identical to `try { ... } catch { ... }`.
*   Using `try` explicitly signals intent (e.g., "this block exists solely to contain risk").

### 3. Statement Catch

For control flow statements like loops, `catch` attaches to the statement body.

#### Resilient Loops

Attaching `catch` to a `for` loop creates a scope for **per-iteration** error handling.

```typescript
for (url in urls) {
  let resume = ExtractResume(url)
  results.append(resume)
} catch {
  e => log.warn("Failed to process url", url)
  // Execution continues to next iteration
}
```

## Semantics

### Return Values

`catch` blocks must return a value compatible with the return type of the scope they attach to.

*   **Expressions**: In BAML, blocks are expressions that evaluate to their last statement. When `catch` is attached, the entire expression evaluates to the union of the block's result and the catch block's result.

```typescript
// resume is inferred as: Resume | null
let resume = { 
  let text = ExtractText(pdf)
  ExtractResume(text) 
} catch { 
  e => null 
}
```

*   **Functions**: The catch block must return a value compatible with the function's return type.
*   **Statements**: For statements that do not yield values (like `for` loops), the catch block does not return a value.

### Pattern Matching

The `catch` clause uses pattern matching syntax, similar to `match` expressions.
*   An untyped pattern like `e` matches any `Error` (but not `Panic`).
*   Specific error types can be matched (e.g., `e: TimeoutError`).

> **Note**: Full details on scoping, panic handling, and complex patterns are available in [02_learn.md](./02_learn.md). This document defines the minimal syntax.

