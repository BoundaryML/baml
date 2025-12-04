# Universal Catch: Guide & Reference

This document serves as both a practical guide and a technical reference for BAML's error handling system, **Universal Catch**.

## Part 1: User Guide

BAML's error handling is designed to be **additive**. You can write simple, optimistic code first, and then "attach" error handling to any part of your code—functions, loops, or individual expressions—without rewriting the logic inside.

### 1. Function-Level Catch
The most common pattern is attaching a catch block to a function. This handles any error that bubbles up from the function body.

```typescript
function ExtractResume(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"Extract resume from {{ text }}"#
} catch {
  // Handle specific errors
  e: TimeoutError => null
  
  // Wildcard catch-all (matches anything except Panic)
  _ => null 
}
```
**Why use it?** It keeps your prompt and client configuration at the top level, preserving IDE features like "Prompt Preview" that rely on declarative syntax.

### 2. Resilient Loops
To prevent a single failure from crashing a batch job, attach `catch` to a loop.

```typescript
for (item in items) {
  let result = Process(item)
  results.append(result)
} catch {
  // Access loop variable 'item' for logging
  e => log.warn(`Failed to process ${item.id}`, e)
  // Execution continues to the next iteration
}
```

### 3. Inline Fallbacks
For simple expressions where you want a default value on failure, use the inline `catch` operator.

```typescript
// If GetScore fails, default to 0
let score = GetScore(resume) catch { _ => 0 }

// Chain of fallbacks
let user = Cache.get(id) catch {
  _ => DB.get(id) catch {
    _ => null
  }
}
```

### 4. Explicit Try Blocks
Use `try` blocks when you need to limit the scope of error handling within a larger function. The `try` keyword is optional but recommended for clarity.

```typescript
function Init() {
  let config = LoadConfig()
  
  // Isolate risky DB connection
  let db = try {
    ConnectDB(config)
  } catch {
    _ => ConnectReplica(config)
  }
  
  return Server(db)
}
```

---

## Part 2: Technical Reference

### 1. Syntax Definition

The `catch` clause can be attached to three syntactic forms.

#### A. Function Declaration
Attaches to the function body.
```typescript
function Name(args) -> ReturnType {
  Body
} catch {
  Handlers
}
```

#### B. Block Expression
Attaches to any `{ ... }` block.
```typescript
// With optional 'try' keyword
try { ... } catch { ... }

// Without keyword (block is an expression)
{ ... } catch { ... }
```

#### C. Expression
Attaches to any expression as a postfix operator.
```typescript
expr catch { ... }
```
*Precedence*: `catch` binds loosely. `a + b catch { ... }` parses as `(a + b) catch { ... }`.

### 2. Exception Hierarchy

BAML distinguishes between **recoverable errors** and **logic bugs (Panics)**.

| Category | Represents | Examples | Default Behavior |
| :--- | :--- | :--- | :--- |
| **Thrown Values** | Recoverable Failures | `TimeoutError`, `BamlClientError`, Custom Types | Caught by `_` wildcard |
| **`Panic`** | Bugs / Logic Errors | `IndexOutOfBounds`, `AssertionError`, `TodoError` | **Propagates** through `_` |

*   **Open Throw System**: You can throw any value. There is no base `Error` type that values must extend.
*   **Panic Definition**: `Panic` is a specific, enumerated set of built-in types representing bugs.
*   **Wildcard Logic**: The wildcard `_` matches any thrown value that is **not** a `Panic`.

#### Built-in Panic Functions

BAML provides built-in functions that throw `Panic` types. These are for marking bugs and incomplete code.

| Function | Throws | Purpose |
| :--- | :--- | :--- |
| `panic(message)` | `PanicError` | Signal an unrecoverable bug. |
| `assert(cond, message)` | `AssertionError` | Validate a runtime invariant. |
| `todo(message)` | `TodoError` | Mark incomplete code. |
| `unreachable(message)` | `UnreachableError` | Mark a code path that should be impossible. |

**Example: Using Panic Functions**
```typescript
function ProcessUser(user_type: string) -> Result {
  if (user_type == "admin") {
    return AdminResult()
  } else if (user_type == "user") {
    return UserResult()
  } else {
    // This should never happen if validation is correct upstream.
    unreachable("user_type must be 'admin' or 'user'")
  }
}

function ValidateScore(score: float) -> float {
  // If the LLM returns a score outside [0, 1], it's a bug in the prompt/parsing.
  assert(score >= 0.0 && score <= 1.0, "Score must be in [0, 1]")
  return score
}

function ExtractResume(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"..."#
} catch {
  e: RateLimitError => todo("Implement retry logic")
  _ => null
}
```

#### Implicit Throw (Panics Propagate by Default)

When a `Panic` is thrown, it is **not** caught by the `_` wildcard. It propagates up the call stack until it either:
1.  Is explicitly caught by a pattern matching a `Panic` type.
2.  Reaches the top of the stack and crashes the program.

This ensures that bugs surface loudly rather than being silently swallowed.

**Example: Panic Propagates Through Wildcard**
```typescript
function Process(items: Item[]) -> Result {
  let first = items[0]  // Throws IndexOutOfBounds if items is empty
  return Transform(first)
} catch {
  _ => DefaultResult()  // Does NOT catch IndexOutOfBounds
}

// If 'items' is empty:
// 1. items[0] throws IndexOutOfBounds (a Panic)
// 2. The wildcard '_' does not match Panic types
// 3. IndexOutOfBounds propagates out of the function
// 4. The program crashes (unless caught by an outer explicit Panic handler)
```

#### Explicitly Catching Panics

To catch a panic, you must explicitly match a `Panic` type. This is intentionally verbose to signal that you are handling a bug, not a normal error.

**Example: Catching a Specific Panic**
```typescript
function SafeGetFirst(items: Item[]) -> Item | null {
  return items[0]
} catch {
  _: IndexOutOfBounds => null  // Explicitly handle this specific panic
}
```

**Example: Catching All Panics (Server Root)**
At the top level of a server, you may want to catch all panics to prevent a crash and log the bug.

```typescript
function ServerEntry() {
  RunApp()
} catch {
  // Explicit Panic handler - catches all bugs
  p: Panic => {
    log.fatal("Critical bug encountered", p)
    // Optionally re-throw, or return an error response
    throw p
  }
  // Wildcard for normal errors
  _ => {
    log.error("Request failed")
    ErrorResponse()
  }
}
```

#### Checked Accessors

To avoid panics from bounds checks, use checked accessors that return `null` instead of panicking.

| Unchecked (Panics) | Checked (Returns `T \| null`) |
| :--- | :--- |
| `array[i]` | `array.get(i)` |
| `map[key]` | `map.get(key)` |

```typescript
// Unchecked: panics if empty
let first = items[0]

// Checked: returns null if empty
let first = items.get(0)
```

### 3. Pattern Matching Rules

The `catch` block uses pattern matching syntax:

```typescript
} catch {
  pattern1 => result1
  pattern2 => result2
}
```

#### Matching Semantics
1.  **First Match**: Patterns are evaluated top-to-bottom. The first matching pattern executes.
2.  **Wildcard (`_`)**: Matches any thrown value **except** `Panic` types.
3.  **Panic Matching**: To catch a panic, you must explicitly match a `Panic` type (e.g., `p: IndexOutOfBounds`).
4.  **Guards (`if`)**: Patterns can include an `if` clause to add a condition.

#### Pattern Guards

A pattern can be followed by an `if` clause to refine when it matches. The guard has access to bound variables from the pattern and variables from the surrounding scope.

```typescript
pattern if condition => result
```

**Example: Conditional Retry**
```typescript
function Fetch(url: string, retryCount: int) -> Response | null {
  client "gpt-4o"
  prompt #"Fetch {{ url }}"#
} catch {
  // Retry on timeout, but only if we haven't exhausted retries
  e: TimeoutError if retryCount < 3 => Fetch(url, retryCount + 1)
  
  // Timeout with no retries left
  e: TimeoutError => null
  
  // All other errors
  _ => null
}
```

**Example: Matching on Error Properties**
```typescript
} catch {
  e: ApiError if e.status == 429 => {
    sleep(e.retryAfter)
    retry()
  }
  e: ApiError if e.status >= 500 => retry()
  e: ApiError => null  // 4xx errors, don't retry
  _ => null
}
```

**Evaluation Order**: A guarded pattern only matches if **both** the type matches **and** the guard evaluates to `true`. If the guard is `false`, evaluation continues to the next pattern.

#### Implicit Desugaring

To enforce the "Panics propagate by default" rule, the compiler performs desugaring on `catch` blocks.

**Rule 1: Wildcards and bare identifiers do not match `Panic`.**

A pattern without a type annotation (e.g., `_`, `e`, `other`) only matches non-Panic values. To match a Panic, you must explicitly annotate with a Panic type.

| Pattern | Matches |
| :--- | :--- |
| `_` | Any non-Panic |
| `e` | Any non-Panic (binds to `e`) |
| `e: TimeoutError` | Only `TimeoutError` |
| `p: Panic` | Any Panic type |
| `p: IndexOutOfBounds` | Only `IndexOutOfBounds` |

**Rule 2: An implicit Panic re-throw is added if no Panic handler exists.**

If the user does not provide a pattern that matches `Panic`, the compiler appends one.

```typescript
// User writes:
catch {
  e: TimeoutError => retry()
  other => log(other)
}

// Compiler desugars to:
catch {
  e: TimeoutError => retry()
  other => log(other)           // 'other' matches non-Panic only
  __implicit_panic: Panic => throw __implicit_panic  // Added by compiler
}
```

**Rule 3: If the user provides a Panic handler, no implicit handler is added.**

```typescript
// User writes (explicit Panic handling):
catch {
  e: TimeoutError => retry()
  p: Panic => {
    log.fatal("Bug!", p)
    throw p
  }
  other => log(other)
}

// Compiler desugars to (no implicit handler needed):
catch {
  e: TimeoutError => retry()
  p: Panic => { log.fatal("Bug!", p); throw p }  // User's Panic handler
  other => log(other)                            // Matches remaining non-Panic
}
```

**Ordering Note**: Because wildcards and `Panic` patterns match disjoint sets (wildcards exclude Panic), their relative order does not matter. A `Panic` will never match a wildcard, so it will always fall through to a `Panic` handler regardless of position. However, for readability, placing explicit type patterns (like `e: TimeoutError`) before wildcards is recommended.

### 4. Scoping and Visibility

#### Variable Access
The code inside a `catch` block can access variables from the **scope immediately surrounding** the attached block.

*   **Function Catch**: Accesses function arguments.
*   **Loop Catch**: Accesses loop variables.
*   **Block Catch**: Accesses variables defined *outside* the block.

**Restriction**: Variables defined *inside* the `try` block are **not** visible in the `catch` block, as they may be uninitialized or dropped.

#### Control Flow
`catch` blocks inside loops or functions can use control flow keywords:
*   `return`: Returns from the enclosing function.
*   `break` / `continue`: Applies to the enclosing loop.

### 5. Type Inference

Since `catch` is an expression, it affects the type of the result.

```typescript
let result = {
  // Returns: T
  Compute() 
} catch {
  // Returns: U
  _ => fallback_value 
}
// Type of 'result' is: T | U
```

*   **Functions**: The union of the body's return type and catch block's return type must be assignable to the function's declared return type.
*   **Loops**: Loop bodies (and their catch blocks) do not return values (type `void`).
