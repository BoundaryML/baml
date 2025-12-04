# Universal Catch: Practical Guide

This document covers common error handling scenarios in BAML.

<!-- TOC_PLACEHOLDER -->

---

## Basic Patterns

### How do I handle errors from a function call?

Attach a `catch` block to the expression:

```typescript
let user = GetUser(id) catch {
  e => null
}
```

The pattern `e` matches any error and binds it to the variable `e`. The variable `user` will be either the result of `GetUser(id)` or `null` if it threw.

### How do I add error handling to an LLM function?

Attach `catch` directly after the function body:

```typescript
function ExtractResume(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"Extract resume from {{ text }}"#
} catch {
  e => null
}
```

The `catch` block attaches to the function itself. No need to wrap the body in a `try` block.

### How do I provide a default value when something fails?

Use inline catch with a fallback value:

```typescript
let score = GetScore(resume) catch { e => 0 }
let name = user.name catch { e => "Unknown" }
```

### How do I chain multiple fallbacks?

Nest catch blocks:

```typescript
let config = LoadFromCache(id) catch {
  e => LoadFromDB(id) catch {
    e => DefaultConfig()
  }
}
```

Each fallback is tried in order. If `LoadFromCache` fails, try `LoadFromDB`. If that fails, use `DefaultConfig()`.

### How does `catch` bind in complex expressions?

`catch` binds loosely—it applies to the entire preceding expression:

```typescript
a + b catch { e => 0 }       // Parses as: (a + b) catch { e => 0 }
Foo().bar catch { e => null } // Parses as: (Foo().bar) catch { e => null }
```

Use parentheses to limit scope:

```typescript
a + (b catch { e => 0 })     // Only 'b' is caught, then added to 'a'
```

### What happens to errors I don't handle?

They propagate to the caller. You don't need to list every error type:

```typescript
function Process(text: string) -> Result {
  let data = Parse(text)  // Can throw ParseError
  Transform(data)         // Can throw TransformError
} catch {
  e: ParseError => DefaultResult()
  // TransformError is not handled here - it propagates up
}
```

The compiler implicitly re-throws unhandled errors. This is equivalent to:

```typescript
} catch {
  e: ParseError => DefaultResult()
  __other => throw __other  // Added by compiler
}
```

### How do I log and re-throw an error?

Use a block in the handler to perform actions before throwing:

```typescript
Process(data) catch {
  e => {
    log.error("Processing failed", e)
    throw e
  }
}
```

---

## Loops & Batch Processing

### How do I continue a loop when one iteration fails?

Attach `catch` to the loop:

```typescript
for (url in urls) {
  let data = Fetch(url)
  results.append(data)
} catch {
  e => log.warn("Failed to fetch", e)
  // Continues to next iteration
}
```

When an error occurs, the handler runs and the loop continues with the next item.

### How do I access the loop variable in the error handler?

Loop variables are in scope inside the catch block:

```typescript
for (item in items) {
  Process(item)
} catch {
  e => log.warn(`Failed to process item ${item.id}`, e)
}
```

---

## Error Discrimination

### How do I handle different error types differently?

Use pattern matching with type annotations:

```typescript
DoWork() catch {
  e: TimeoutError => Retry()
  e: NetworkError => FallbackResult()
  // Other errors implicitly propagate to caller
}
```

Patterns are evaluated top-to-bottom. The first match wins. Unhandled errors propagate automatically.

### How do I match on error properties (like status codes)?

Use pattern guards with `if`:

```typescript
CallAPI() catch {
  e: ApiError if e.status == 404 => null
  e: ApiError if e.status >= 500 => Retry()
  e: ApiError => DefaultResult()
  // Non-ApiError errors propagate
}
```

The guard condition has access to the bound variable `e`.

### How do I catch a union of error types?

You can match multiple types in a single pattern using `|`:

```typescript
Fetch(url) catch {
  e: TimeoutError | ConnectionError | DNSError => fallbackFetch(url)
  e => null
}
```

Alternatively, you can define a type alias:

```typescript
type NetworkIssue = TimeoutError | ConnectionError | DNSError

Fetch(url) catch {
  e: NetworkIssue => fallbackFetch(url)
  e => null
}
```

Or match each type separately:

```typescript
Fetch(url) catch {
  e: TimeoutError => fallbackFetch(url)
  e: ConnectionError => fallbackFetch(url)
  e: DNSError => fallbackFetch(url)
  e => null
}
```

Or match against the union inline:

```typescript
Fetch(url) catch {
  e: TimeoutError | ConnectionError | DNSError => fallbackFetch(url)
  e => null
}
```

---

## Panics vs Errors

### What's the difference between an Error and a Panic?

| Category | Represents | Examples | Caught by untyped pattern? |
|:---------|:-----------|:---------|:---------------------------|
| **Error** | Recoverable failures | `TimeoutError`, `NetworkError`, custom types | Yes |
| **Panic** | Bugs / logic errors | `IndexOutOfBounds`, `AssertionError` | No |

Errors are expected failure modes your code should handle. Panics indicate bugs that should crash the program.

### Why doesn't my catch block catch `IndexOutOfBounds`?

`IndexOutOfBounds` is a Panic, not an Error. Untyped patterns like `e` only catch Errors:

```typescript
function GetFirst(items: Item[]) -> Item {
  return items[0]  // Throws IndexOutOfBounds if empty
} catch {
  e => DefaultItem()  // Does NOT catch IndexOutOfBounds
}
```

If `items` is empty, `IndexOutOfBounds` propagates through the catch block and crashes the program.

### How do I safely access array/map elements without panics?

Use checked accessors that return `null` instead of panicking:

| Unchecked (panics) | Checked (returns `T \| null`) |
|:-------------------|:------------------------------|
| `array[i]` | `array.get(i)` |
| `map[key]` | `map.get(key)` |

```typescript
let first = items.get(0)  // Returns null if empty, no panic
if (first != null) {
  Process(first)
}
```

### How do I explicitly catch a Panic when I need to?

Match on a specific Panic type or the `Panic` union type.

Note that untyped patterns like `e` do **not** match Panics. If you want to log *all* failures (bugs and errors), you must handle Panics explicitly:

```typescript
RunApp() catch {
  // 1. Handle Bugs (Panics)
  p: Panic => {
    log.fatal("Bug encountered", p)
    throw p
  }
  
  // 2. Handle Recoverable Errors
  e => {
    log.error("Request failed", e)
    ErrorResponse()
  }
}
```

Catching panics should be rare. It's usually better to fix the bug or use checked accessors.

### How do I catch a specific Panic?

Match on the specific type:

```typescript
items[0] catch {
  // Only catches index errors
  p: IndexOutOfBounds => DefaultItem()
  // Other panics (like AssertionError) still crash the program
}
```

If you only catch a specific Panic type, the compiler still adds an implicit handler for the remaining Panic types.

### What are the Panic types?

`Panic` is a union of these built-in types:

**Collection Access**

| Type | Thrown By | Cause |
|:-----|:----------|:------|
| `IndexOutOfBounds` | `array[i]` | Invalid index |
| `KeyNotFound` | `map[key]` | Missing key |

**Development Markers**

| Type | Thrown By | Cause |
|:-----|:----------|:------|
| `TodoError` | `todo()` | Incomplete code executed |
| `UnreachableError` | `unreachable()` | "Impossible" path reached |
| `AssertionError` | `assert()` | Assertion failed |
| `PanicError` | `panic()` | Generic fatal error |

**Arithmetic**

| Type | Thrown By | Cause |
|:-----|:----------|:------|
| `DivisionByZero` | `a / b`, `a % b` | Divisor is zero |
| `IntegerOverflow` | `a + b`, `a * b`, etc. | Result exceeds bounds |

**Runtime**

| Type | Thrown By | Cause |
|:-----|:----------|:------|
| `StackOverflow` | Recursive calls | Recursion limit exceeded |

You can match on individual types or the full `Panic` union.

### Can I throw strings or custom types?

Yes. BAML has an open throw system. You can throw any value:

```typescript
throw "Invalid state"
throw { code: 500, msg: "Error" }
```

These are treated as **Errors** (recoverable) and are caught by the `_` wildcard. They are not Panics.

---

## Signaling Bugs

BAML provides built-in functions that throw Panics to mark bugs and incomplete code.

### How do I mark code as incomplete? (`todo`)

```typescript
function HandleRateLimit() -> Response {
  todo("Implement rate limit handling")
}
```

`todo()` throws `TodoError`. Use it as a placeholder during development.

### How do I assert invariants? (`assert`)

```typescript
function ValidateScore(score: float) -> float {
  assert(score >= 0.0 && score <= 1.0, "Score must be in [0, 1]")
  return score
}
```

`assert()` throws `AssertionError` if the condition is false.

### How do I mark unreachable code paths? (`unreachable`, `panic`)

```typescript
function ProcessType(t: string) -> Result {
  if (t == "a") {
    return handleA()
  } else if (t == "b") {
    return handleB()
  } else {
    unreachable("Type must be 'a' or 'b'")
  }
}
```

`unreachable()` throws `UnreachableError`. Use it for code paths that should never execute.

For general unrecoverable bugs, use `panic()`:

```typescript
panic("Something went very wrong")
```

---

## Scoping

### How do I limit the scope of error handling within a function?

Use a block expression with catch:

```typescript
function Init() -> Server {
  let config = LoadConfig()
  
  let db = {
    ConnectDB(config)
  } catch {
    e => ConnectReplica(config)
  }
  
  return Server(db)
}
```

The `try` keyword is optional but can clarify intent:

```typescript
let db = try {
  ConnectDB(config)
} catch {
  e => ConnectReplica(config)
}
```

Both forms are semantically identical.

### What variables can I access in my catch block?

You can access variables from the scope surrounding the attached block:

- **Function catch**: Function arguments
- **Loop catch**: Loop variables
- **Block catch**: Variables defined outside the block

You cannot access variables defined inside the try block:

```typescript
{
  let temp = Compute()  // Defined inside
  UseTemp(temp)
} catch {
  e => log(temp)  // Error: 'temp' is not accessible
}
```

Variables inside the block may be uninitialized when an error occurs, so they're not available in the handler.

### How do I run cleanup code (finally)?

BAML does not have a `finally` block. Place cleanup code after the catch expression:

```typescript
let resource = Acquire()

let result = {
  Use(resource)
} catch {
  e => null
}

Release(resource)  // Runs after success or caught error
```

---

## Type System

### How does `catch` affect my return type?

The result type is the union of the try expression's type and each handler's return type:

```typescript
// result is: Resume | null
let result = ExtractResume(text) catch {
  e => null
}

// result is: int
let result = ComputeScore() catch {
  e => 0  // Same type as success case
}

// result is: Data | DefaultData | null
let result = FetchData() catch {
  e: NetworkError => DefaultData()
  e => null
}
```

**Handlers must return a value.** A handler that only performs side effects is a compile error:

```typescript
// ❌ Compile error: handler must return a value
let result = ExtractResume(text) catch {
  e => log(e)  // log() returns void, not Resume | null
}

// ✅ Log and return a fallback
let result = ExtractResume(text) catch {
  e => {
    log(e)
    null
  }
}
```

**For functions, the catch return type must match the declared return type:**

```typescript
// ❌ Compile error: handler returns wrong type
function Extract(text: string) -> Resume {
  client "gpt-4o"
  prompt #"..."#
} catch {
  e => null  // Error: 'null' is not assignable to 'Resume'
}

// ✅ Widen return type to include fallback
function Extract(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"..."#
} catch {
  e => null  // OK: null is part of return type
}
```

### How does the compiler desugar my catch block?

The compiler adds implicit handlers to propagate unhandled errors and panics:

```typescript
// You write:
DoWork() catch {
  e: TimeoutError => null
}

// Compiler produces:
DoWork() catch {
  e: TimeoutError => null
  __implicit_panic: Panic => throw __implicit_panic // Re-throw all Panics
  __implicit_error => throw __implicit_error        // Re-throw unhandled Errors
}
```

If you add a catch-all pattern, only the panic handler is added:

```typescript
// You write:
DoWork() catch {
  e: TimeoutError => null
  e => DefaultResult()  // Catch-all for remaining errors
}

// Compiler produces:
DoWork() catch {
  e: TimeoutError => null
  __implicit_panic: Panic => throw __implicit_panic // Inserted before catch-all
  e => DefaultResult()
}
```

If you explicitly handle `Panic`, no implicit panic handler is added:

```typescript
// You write:
DoWork() catch {
  p: Panic => handleBug(p)
  e => null
}

// No implicit handlers added - you've handled everything explicitly
```

Untyped patterns (like `e`) do not match Panic types. To catch a Panic, you must annotate with a Panic type explicitly.
