# Design Alternatives

This document explains why we chose Universal Catch over other error handling designs.

---

## Rejected Designs

### Why not use Result types like Rust?

```typescript
function Extract(text: string) -> Result<Resume, Error> { ... }

let result = Extract(text)
match result {
  Ok(resume) => ...
  Err(e) => ...
}
```

**Rejected because:**

- **All or Nothing**: Result types work best when used consistently across the codebase. Mixing Result-returning functions with throwing functions creates friction at every boundary.
- **Viral Complexity**: Changing a return type to `Result` forces all callers to update their signatures to handle or propagate it.

---

### Why not use classic `try/catch` blocks?

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
    // Confusing: Are we "trying" to define the client?
    client "gpt-4o"
    prompt #"Extract resume from {{ text }}"#
  } catch {
    e: TimeoutError => null
  }
}
```

**Rejected because:**

- **Indentation Tax**: Wrapping in `try` re-indents every line, breaking git blame and inflating diffs.
- **Hoisting Tax**: Variables declared inside `try` are not accessible in `catch` or after the block, forcing declarations to be moved outside.
- **Declarative Incompatibility**: `try` implies sequential execution. Wrapping declarative properties like `client` and `prompt` in an imperative block creates a semantic mismatch.

---

### Why not make `try` an expression (like Kotlin)?

```typescript
let resume = try {
  Extract(text)
} catch {
  e => null
}
```

**Rejected because:**

- **Partial Solution**: Solves the variable hoisting issue but doesn't work for declarative code. You can't wrap `client` and `prompt` declarations in a try expression.

---

### Why not use function modifiers?

```typescript
function Extract(text: string) -> Resume try {
  client "gpt-4"
  prompt #"..."#
} catch {
  e => null
}
```

**Rejected because:**

- **Syntactic Irregularity**: Introduces a special grammar rule that doesn't compose with other constructs.

---

### Why not use wrapper functions?

```typescript
function Extract(text: string) -> Resume | null {
  try {
    return _ExtractInternal(text)
  } catch {
    return null
  }
}
```

**Rejected because:**

- **Boilerplate**: Doubles the function count for simple error handling.
- **Tooling Degradation**: Breaks the link between the prompt definition and the execution context (e.g., "Prompt Preview" or "Run Function" features).
- **Cognitive Load**: Developers must manage and recall two versions of every function.

---

### Why not use checked exceptions like Java?

```typescript
function Extract(text: string) -> Resume throws TimeoutError, ParseError { ... }
```

**Rejected because:**

- **Virality**: Adding a new error type to a low-level function forces signature updates to every caller in the stack. In practice, teams declare `throws Error` everywhere to avoid maintenance, defeating the purpose.

---

### Why pattern matching syntax in catch?

**vs `catch (e) { if/instanceof }`:**

```typescript
// Traditional: bind one variable, discriminate inside
try { Extract(text) }
catch (e) {
  if (e instanceof TimeoutError) { retry() }
  else if (e instanceof ParseError) { return null }
  else { throw e }
}

// Pattern matching: discrimination in the syntax
Extract(text) catch {
  e: TimeoutError => retry()
  e: ParseError => null
}
```

**vs chained catch blocks (Java):**

```typescript
try { Extract(text) }
catch (e: TimeoutError) { retry() }
catch (e: ParseError) { return null }
```

With chained blocks, catching "everything else" means `catch (e) { ... }`, which catches panics too. No way to let bugs propagate.

**Why untyped patterns exclude Panics:**

In `match`, an untyped pattern matches everything. In `catch`, an untyped pattern like `e` matches all Errors but not Panics. Bugs crash loudly by default. To catch panics: `p: Panic => ...`.

**Why implicit re-throw:**

Unhandled cases propagate automatically. Start by handling one error type, add more as you harden. No `else { throw e }` boilerplate.

**Trade-off: log + rethrow is slightly more verbose:**

```typescript
// Traditional catch (e) - concise for log + rethrow
catch (e) {
  log(e)
  throw e
}

// Pattern matching - requires a block
catch {
  e => {
    log(e)
    throw e
  }
}
```

But without pattern matching, you can't distinguish Errors from Panics:

```typescript
// Traditional: catches EVERYTHING, including bugs
catch (e) {
  return defaultValue  // Swallows IndexOutOfBounds, AssertionError...
}

// Pattern matching: only catches recoverable errors
catch {
  e => defaultValue
  // IndexOutOfBounds, AssertionError crash immediately - not caught
}

// To handle panics too, be explicit:
catch {
  p: Panic => {
    log.fatal(p)
    throw p
  }
  e => defaultValue
}
```

The slight verbosity is the cost of having untyped patterns mean "all errors except bugs."
