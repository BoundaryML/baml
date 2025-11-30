# Deep Dive: Go Error Handling

## Core Philosophy: Errors are Values
Go treats errors as first-class values, not as control flow exceptions. This is a deliberate design choice to prioritize **explicitness** and **simplicity** over conciseness.

The `error` interface is minimal:
```go
type error interface {
    Error() string
}
```

## Developer Experience (DX)

### The "Happy Path" vs. Error Handling
Go code often exhibits a "left-aligned" happy path. Errors are handled immediately, usually resulting in a return.

```go
func processUser(id string) (*User, error) {
    user, err := db.GetUser(id)
    if err != nil {
        return nil, fmt.Errorf("failed to get user: %w", err)
    }

    if err := validateUser(user); err != nil {
        return nil, fmt.Errorf("validation failed: %w", err)
    }

    return user, nil
}
```

**DX Pros**:

- **Local Reasoning**: You know exactly where control flows. No hidden jumps.
- **No Surprise Exceptions**: Functions signature tells you if it can fail (returns `error`).

**DX Cons**:

- **Verbosity**: The `if err != nil` pattern is repetitive (often 50% of lines).
- **Shadowing**: Frequent use of `err` variable can lead to accidental shadowing bugs.

### Error Wrapping and Inspection
Since Go 1.13, the standard library supports error wrapping.

**Wrapping**:
```go
// Adds context while preserving the original error type for inspection
return fmt.Errorf("access denied for user %s: %w", uid, errPermissionDenied)
```

**Inspection (`errors.Is` / `errors.As`)**:
Instead of `==` or type assertions, use:
```go
if errors.Is(err, os.ErrNotExist) {
    // Handle file not found
}

var pathErr *os.PathError
if errors.As(err, &pathErr) {
    // Access pathErr.Path
}
```

## Implementation Tradeoffs

### 1. Stack Traces vs. Performance
**Tradeoff**: Go errors are lightweight (just an interface value).

- **Benefit**: Extremely low overhead. Creating an error is just allocating a small struct.
- **Cost**: **No stack traces by default**. Debugging where an error originated requires manual context adding (wrapping) at every stack frame, or using libraries like `pkg/errors` (deprecated but popular) that attach stack traces, which adds allocation overhead.

### 2. Control Flow
**Tradeoff**: No exceptions means no "jump up the stack".

- **Benefit**: Control flow is obvious. `defer` is the only mechanism that runs on return.
- **Cost**: You cannot easily "abort" a deep operation without checking error returns at every single level. `panic` exists but is reserved for truly unrecoverable state (like nil pointer dereference), not operational errors.

### 3. Sentinel Errors vs. Custom Types
- **Sentinel Errors** (`var ErrNotFound = errors.New("...")`): Fast `==` checks, but tight coupling to specific values.
- **Custom Types**: More context, but requires type assertions/`errors.As`.

## The Role of `defer` in Error Handling

`defer` is Go's mechanism for guaranteed cleanup, and it plays a crucial role in error handling patterns. A deferred function call is executed when the surrounding function returns, **regardless of whether that return is normal or via panic**.

### Pattern 1: Resource Cleanup with Error Propagation

The most common pattern is closing resources (files, connections, locks) while ensuring errors don't get lost:

```go
func processFile(path string) (err error) {
    f, err := os.Open(path)
    if err != nil {
        return fmt.Errorf("failed to open: %w", err)
    }
    defer func() {
        closeErr := f.Close()
        if closeErr != nil && err == nil {
            // Only override if no error exists yet
            err = fmt.Errorf("failed to close: %w", closeErr)
        }
    }()
    
    // Work with f...
    return processData(f)
}
```

**Key Insight**: Using a **named return value** (`err error`) allows the deferred function to modify the return error. This is idiomatic in Go for resource cleanup.

**DX Consideration**: 

- **Pro**: Cleanup is guaranteed and colocated with acquisition.
- **Con**: Subtle bugs if you forget to use named returns or accidentally shadow `err` inside the defer.

### Pattern 2: Adding Context on Error

Defer can wrap errors with additional context just before returning:

```go
func updateUser(id string, data UserData) (err error) {
    defer func() {
        if err != nil {
            err = fmt.Errorf("updateUser(id=%s): %w", id, err)
        }
    }()
    
    // Multiple operations, any might fail
    user, err := db.GetUser(id)
    if err != nil {
        return err
    }
    
    user.Update(data)
    return db.SaveUser(user)
}
```

This avoids repeating context at every error return site.

### Pattern 3: `defer`/`recover` for Panic Handling

Go's `panic` is analogous to exceptions but reserved for truly exceptional cases (programmer errors, unrecoverable state). `recover()` can catch panics, but **only when called from within a deferred function**:

```go
func safeHandler(w http.ResponseWriter, r *http.Request) {
    defer func() {
        if recovered := recover(); recovered != nil {
            log.Printf("panic recovered: %v", recovered)
            http.Error(w, "Internal Server Error", 500)
        }
    }()
    
    // Code that might panic (e.g., nil pointer dereference)
    riskyOperation()
}
```

**When to Use**:

- **Top-level handlers** (HTTP handlers, goroutine entry points) to prevent crashes.
- **NOT for normal error handling** – overuse makes control flow implicit.

**DX Tradeoff**:

- **Benefit**: Catches unexpected panics at boundaries (e.g., between user code and framework).
- **Cost**: Adds hidden control flow. Go culture strongly discourages using panic/recover for expected errors.

### Pattern 4: Transaction Rollback

Defer is often used with database transactions:

```go
func createOrder(ctx context.Context, order Order) (err error) {
    tx, err := db.BeginTx(ctx, nil)
    if err != nil {
        return err
    }
    defer func() {
        if err != nil {
            tx.Rollback() // Rollback on any error
        } else {
            err = tx.Commit() // Commit and capture commit errors
        }
    }()
    
    if err = tx.InsertOrder(order); err != nil {
        return err
    }
    if err = tx.UpdateInventory(order.Items); err != nil {
        return err
    }
    return nil // Commit happens in defer
}
```

### Defer Execution Order

Defers execute in **LIFO (last-in, first-out)** order:

```go
func example() {
    defer fmt.Println("1")
    defer fmt.Println("2")
    defer fmt.Println("3")
    // Prints: 3, 2, 1
}
```

This matters when managing nested resources:

```go
func processFiles(paths []string) error {
    for _, path := range paths {
        f, err := os.Open(path)
        if err != nil {
            return err
        }
        defer f.Close() // ⚠️ BUG: All files close at function end, not loop iteration
    }
    return nil
}

// Fix: Use a separate function to ensure defer runs per iteration
func processFiles(paths []string) error {
    for _, path := range paths {
        if err := processOneFile(path); err != nil {
            return err
        }
    }
    return nil
}

func processOneFile(path string) error {
    f, err := os.Open(path)
    if err != nil {
        return err
    }
    defer f.Close() // ✓ Closes after this iteration
    // Process file...
    return nil
}
```

### Performance Considerations

- **Defer overhead**: Small but non-zero (function call + defer metadata). In Go 1.14+, defer is much faster (~1.8ns overhead) but still slower than inline code.
- **Hot paths**: Some performance-critical code avoids defer and does explicit cleanup before each return.

### Comparison to Other Languages

| Language | Cleanup Mechanism | Execution Guarantee |
|----------|------------------|---------------------|
| Go | `defer` | On return or panic |
| Python | `with` / `finally` | On block exit or exception |
| Rust | `Drop` trait | On scope exit (RAII) |
| Java | `try-with-resources` | On try block exit |
| C++ | Destructors (RAII) | On scope exit |

Go's `defer` is **explicit** (you see the defer call) but **order-dependent** (LIFO can be surprising). Rust's RAII is implicit but deterministic.

## Summary
Go optimizes for **readability of control flow** at the expense of **write-time verbosity**. It forces developers to consider failure states at every step. `defer` provides a powerful mechanism for guaranteed cleanup and error propagation, but requires understanding of named returns and execution order to use correctly.