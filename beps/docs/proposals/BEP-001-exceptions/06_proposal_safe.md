# Safe Functions

This document proposes the `safe` keyword to enable local reasoning about error handling and strictly enforce exhaustiveness.

## Background: The Need for Local Reasoning

While Universal Catch (BEP-001) improves the ergonomics of error handling, it leaves two significant gaps regarding correctness and composition:

1. **Lack of Local Reasoning**: By default, BAML functions use implicit re-throws for unhandled errors. To know if a function `Extract()` is safe to call, a developer (or AI Agent) must read its source code or rely on IDE inference. There is no way to declare "this function handles all its own errors" in the signature itself.

2. **Optional Exhaustiveness**: For prototyping speed, BAML does not force developers to handle every possible error type. However, for reliable systems, we need a mechanism to enforce that *all* known recoverable errors are handled, ensuring no expected failure modes are accidentally ignored.

We need a way to transform the *implicit* behavior of exceptions into an *explicit* guarantee of safety, without the verbosity of Java's checked exceptions or Rust's `Result` types.

## Proposal

We introduce the `safe` keyword, which acts as a contract that a scope (function or expression) handles all recoverable `Error` types.

### 1. Safe Functions

A function declared as `safe` guarantees that no `Error` types escape its body.

```typescript
safe function Extract(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"..."#
} catch {
  // Compiler ENFORCES that these handlers are exhaustive for Error
  e: TimeoutError => null
  e: RefusalError => null
  // If we missed 'NetworkError', this fails to compile.
  // Wildcard '_' satisfies exhaustiveness immediately.
}
```

**Semantics**:

* **Strict Exhaustiveness**: The compiler disables implicit re-throws for `Error` types. Every error the body can throw must be matched.
* **Panic Propagation**: `safe` guarantees the absence of `Error`. It does *not* catch `Panic` (bugs). A `safe` function can still crash if a bug occurs (e.g., `assert` fails).

### 2. Safe Expressions

The `safe` keyword can enforce handling at the call site for any expression, including blocks.

```typescript
// Compiler ensures the attached catch block is exhaustive
let user = safe GetUser(id) catch {
  _ => null 
}

// Applying safe to a block enforces strict handling for the entire block
// This is similar to a try-catch, but with mandatory exhaustiveness for Errors
let result = safe {
  let x = Compute(data)
  Process(x)
} catch {
  e: CalculationError => 0
  _ => -1 
}
```

### 3. Safety Inference

The compiler automatically infers a function as "semantically safe" if it handles all errors, even without the keyword. However, adding the `safe` keyword makes this a **checked contract**: if the implementation changes to introduce a new unhandled error, the compiler will error.

## Alternatives

### Checked Exceptions (`throws`)

Java requires functions to declare what they throw (`throws IOException`). This pushes the burden to the *caller*.

* **Decision**: Rejected. We prefer pushing the burden to the *callee* (the `safe` function) to contain errors, or using `safe` at the call site to explicitly handle them.

### Result Types (`Result<T, E>`)

Rust uses `Result` types.

* **Decision**: Rejected. `safe` allows us to keep direct return types (`Resume | null`) while still guaranteeing safety, which is more ergonomic for the "AI Engineering" domain.

## What `safe` Enables

### 1. Agent-Readable Contracts

AI Agents writing BAML code can look at a function signature:

* `function DoWork()` -> "I might need to wrap this in a catch block."
* `safe function DoWork()` -> "I can call this directly; it handles its own failures."

### 2. Refactoring Confidence

If `Extract` is modified to throw a new `BamlClientError`:

* **Without `safe`**: The error silently propagates up the stack, potentially crashing a distant caller.
* **With `safe`**: The `safe function` failing to handle the new error triggers a compile-time error immediately.

### 3. Incremental Adoption

The design supports the full software lifecycle:

1. **Prototype**: Write normal functions. Let errors propagate or use `_`.
2. **Production**: Add `safe` to critical functions. The compiler forces you to handle edge cases you missed.

## Tooling Implications

The explicit `safe` contract enables tooling that was not possible with implicit error propagation.

### 1. Visual Safety Indicators

IDEs can display warnings in the gutter when a function contains unhandled unsafe calls:

```text
  1 │   function ProcessBatch(texts: string[]) -> Report {
  2 │     let results = []
  3 │     for (text in texts) {
  4 │ ⚠     let resume = Extract(text)       // unsafe: can throw LLMError
  5 │       results.append(resume)
  6 │     }
  7 │ ⚠   let summary = Summarize(results)   // unsafe: can throw LLMError
  8 │     return Report { results, summary }
  9 │   }
```

Hovering over the `⚠` shows which errors can propagate:

```text
⚠ Extract(text) can throw: LLMError, TimeoutError, ParseError
  
  Add error handling:
    Extract(text) catch { _ => defaultResume }
```

This surfaces unsafe operations without requiring developers to trace through call graphs.

### 2. Inline Diagnostics

The compiler can flag unsafe calls inside `safe` functions:

```typescript
safe function Process() -> Result {
   let x = Extract(text)
   //      ~~~~~~~~~~~~~ error: unsafe call in safe function
   //      Extract() can throw LLMError, TimeoutError
   //      hint: add `catch { ... }` or use `safe Extract(text) catch { ... }`
   
   let y = Extract(text) catch { _ => default }
   //      OK: error handling present
}
```

### 3. Agent Metadata

Generated client code includes safety metadata for AI agents and other tooling:

```typescript
// baml_client/metadata.ts
export const BAML_FUNCTION_METADATA = {
  Extract: {
    isSafe: false,
    canThrow: ["LLMError", "TimeoutError", "ParseError"],
  },
  SafeExtract: {
    isSafe: true,
    canThrow: [],
  },
  FormatName: {
    isSafe: true,
    canThrow: [],
  }
}
```

An agent can query this before generating code:

```typescript
if (BAML_FUNCTION_METADATA.Extract.isSafe) {
  // Call directly
  return `Extract(text)`
} else {
  // Wrap in error handling
  return `Extract(text) catch { _ => null }`
}
```

### 4. CLI Safety Analysis

```bash
$ baml safety graph --function ProcessBatch

ProcessBatch (unsafe)
├─⚠ Extract (unsafe)
│  └─ client "openai/gpt-4o"
├─⚠ Summarize (unsafe) 
│  └─ client "openai/gpt-4o"
└─● FormatReport (safe)

Legend:
  ● = safe    ⚠ = unsafe    [caught] = error handling present
```

With error handling added:

```bash
$ baml safety graph --function ProcessBatch

ProcessBatch (safe)
├─⚠ Extract (unsafe) [caught]
│  └─ client "openai/gpt-4o"
├─⚠ Summarize (unsafe) [caught]
│  └─ client "openai/gpt-4o"
└─● FormatReport (safe)
```

The tree view shows exactly where errors originate and whether they are handled.
