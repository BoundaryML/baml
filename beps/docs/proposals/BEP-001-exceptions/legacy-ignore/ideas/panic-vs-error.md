# Panic vs. Error: Two Classes of Exceptions

**Date**: 2025-12-03

## Motivation

Drawing inspiration from Rust, we want to distinguish between two fundamentally different classes of exceptions:

1. **Recoverable Errors**: Expected runtime conditions (LLM timeouts, network failures, parsing errors)
2. **Logic Errors/Bugs**: Programmer mistakes (array index out of bounds, failed assertions, incomplete code)

Languages like Rust provide special constructs for the latter category:

- `panic!()` - Something went terribly wrong
- `todo!()` - Mark incomplete implementations  
- `unreachable!()` - Document impossible code paths
- `assert!()` - Validate invariants

These serve different purposes than regular error handling:

- They help during development (marking incomplete work, ruling out invariants)
- They catch bugs early (failed assertions, unreachable code being reached)
- They're not meant to be routinely caught and handled

## The Proposal: Error and Panic Unions

Since BAML supports union subtyping (`A` is a subtype of `A | B`), we can model this distinction using union types:

```typescript
// Individual exception types (concrete classes)
TimeoutError
ParseError
NetworkError
IndexOutOfBoundsError
TodoError
AssertionError  
UnreachableError

// Union type aliases
type Error = TimeoutError | ParseError | NetworkError | ...
type Panic = IndexOutOfBoundsError | TodoError | AssertionError | UnreachableError | ...
type Exception = Error | Panic
```

**Key principle**: `Panic` represents bugs. `Error` represents expected failures.

## Wildcard Semantics

The critical design decision: **Wildcards in `catch` blocks have type `Error`, not `Exception`.**

This means you cannot accidentally catch panics—you must be explicit.

### Example 1: Default Behavior (Panics Propagate)

```typescript
function Process(items: Item[]) -> Result {
  let first = items[0]  // Can throw IndexOutOfBoundsError (a Panic)
  return TransformItem(first)
} catch {
  e: TimeoutError => retry()
  _ => DefaultResult()  // Wildcard matches only Error, not Panic
}

// Desugars to:
} catch {
  e: TimeoutError => retry()
  _: Error => DefaultResult()
  /* implicit */
  _p: Panic => throw _p
}
```

If `items` is empty, the `IndexOutOfBoundsError` propagates up—it's not caught by the wildcard.

### Example 2: Explicitly Handling Panics

```typescript
function DefensiveProcess(items: Item[]) -> Result {
  let first = items[0]
  return TransformItem(first)  
} catch {
  p: Panic => {
    log.fatal("Panic occurred", p)
    throw p  // Or handle it
  }
  e: Error => DefaultResult()
}
```

To catch panics, you **must** explicitly pattern match on `Panic` (or specific panic types).

### Example 3: Catching Everything

To catch both errors and panics, you need **two patterns**:

```typescript
} catch {
  p: Panic => handlePanic(p)
  e: Error => handleError(e)
}
```

You **cannot** write:
```typescript
} catch {
  everything: Exception => handle(everything)  // ❌ Not allowed
}
```

This forces developers to think about panics separately from errors.

## Built-in Panic-Throwing Functions

### `assert(condition: bool, message: string)`

Validates runtime invariants. Throws `AssertionError` (a `Panic`) if the condition is false.

```typescript
function ValidateScore(score: float) -> float {
  assert(score >= 0.0 && score <= 1.0, "Score must be in [0, 1]")
  return score
}
```

**Use case**: Validating LLM outputs against known constraints.

### `todo(message: string) -> T`

Marks incomplete implementations. Throws `TodoError` (a `Panic`).

```typescript
function ExtractResume(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"Extract resume from {{ text }}"#
} catch {
  e: RateLimitError => todo("Implement retry logic with exponential backoff")
  e: TimeoutError => null
}
```

**Use case**: Prototype to production workflow—mark areas to revisit.

### `unreachable(message: string) -> T`

Documents code paths that should be impossible. Throws `UnreachableError` (a `Panic`).

```typescript
function ProcessUser(user_type: string) -> Result {
  if (user_type == "admin") {
    return AdminResult()
  } else if (user_type == "user") {
    return UserResult()
  } else {
    unreachable("user_type must be 'admin' or 'user' (validated upstream)")
  }
}
```

**Use case**: Document assumptions about control flow.

## Safe Accessors

To avoid panics, provide safe alternatives that return optionals:

```typescript
// Unsafe (panics on out of bounds)
let first = items[0]  // Throws IndexOutOfBoundsError

// Safe (returns optional)
let first = items.get(0)         // Returns Item | null
let first = items.first()        // Returns Item | null  

// With error handling
let first = items.get(0) catch { _ => DefaultItem() }
```

**Design principle**:

- `array[i]` is for when you **know** `i` is in bounds (assertion of invariant)
- `array.get(i)` is for when it **might** be out of bounds (defensive programming)

## Type Checker Behavior

The type checker does **not** require exhaustiveness over `Panic` types.

```typescript
// ✅ Valid: You don't need to handle TodoError, AssertionError, etc.
} catch {
  e: TimeoutError => null
}
```

The implicit desugaring adds the panic re-throw, so panics always propagate unless explicitly caught.

## Benefits

1. **Prevents catching bugs**: Wildcards can't accidentally swallow panics
2. **Clear intent**: Code that handles panics is explicit about defensive programming
3. **Development ergonomics**: `todo()`, `assert()`, `unreachable()` help during prototyping
4. **Aligns with "prototype to production"**: Mark incomplete work, then replace with proper error handling

## Open Questions

1. Should there be a lint that warns when catching `Panic` in production code?
2. Should `todo()` be a compile error in production builds?
3. Should certain panics (like `TodoError`) be uncatchable in production?
4. How do we handle the `array[i]` vs `array.get(i)` distinction in error messages?

## Interaction with Safe Functions

This proposal has important implications for the `safe` keyword from [safe-unsafe-coloring.md](./safe-unsafe-coloring.md).

### What Does "Safe" Mean?

A `safe` function or expression guarantees that it handles all **`Error`** types, but **not** `Panic` types.

**Rationale**: Panics represent bugs (programmer mistakes), not expected runtime failures. A "safe" function means "won't throw expected errors," but bugs can still surface as panics.

### Safe Expressions and Wildcards

When using the `safe` keyword at a call site, wildcards only need to cover `Error` types:

```typescript
// ✅ Valid: safe with wildcard catches all Errors
let x = safe GetData() catch { _ => null }

// Panics still propagate (by design)
// If GetData() throws IndexOutOfBoundsError, it escapes
```

**Desugaring**:
```typescript
let x = safe GetData() catch { _ => null }

// Becomes:
let x = GetData() catch {
  _: Error => null
  /* implicit */
  _p: Panic => throw _p
}
```

The `safe` keyword ensures all `Error` types are handled, but panics are allowed to propagate.

### Safe Functions and Panics

A function declared as `safe` must handle all `Error` types, but **can** throw `Panic`:

```typescript
// ✅ Valid: safe function can panic
safe function Process(items: Item[]) -> Result {
  let first = items[0]  // Can throw IndexOutOfBoundsError (Panic)
  return TransformItem(first)
} catch {
  e: TimeoutError => retry()
  _ => DefaultResult()  // Handles all Errors
}
// Panics are allowed to escape
```

If you want a function that truly cannot throw **anything** (including panics), you must explicitly handle them:

```typescript
// Truly panic-proof function
safe function DefensiveProcess(items: Item[]) -> Result {
  let first = items.get(0) catch { _ => null }  // Use safe accessor
  if (first == null) {
    return DefaultResult()
  }
  return TransformItem(first)
} catch {
  e: Error => DefaultResult()
}
// No panics possible - uses safe accessors
```

### Exhaustiveness Checking

The type checker's exhaustiveness checking for `safe` expressions only considers `Error` types:

```typescript
// ✅ Valid: wildcard is exhaustive over Error
let x = safe GetData() catch { _ => null }

// ✅ Valid: explicit Error patterns are exhaustive
let x = safe GetData() catch {
  e: TimeoutError => retry()
  e: ParseError => null
  e: NetworkError => null
  _ => null  // Catches remaining Errors
}

// ❌ Compile error: not exhaustive over Error
let x = safe GetData() catch {
  e: TimeoutError => null
  // Missing other Error types and no wildcard
}
```

To catch panics in a `safe` expression, you must be explicit:

```typescript
// Catch both Errors and Panics
let x = safe GetData() catch {
  p: Panic => {
    log.fatal("Unexpected panic", p)
    throw p  // Or handle it
  }
  e: Error => null
}
```

### Safe Function Inference

A function is inferred as safe if:

1. It has no unsafe operations, OR
2. All unsafe operations are wrapped in `catch` blocks that handle all `Error` types

Panics do **not** affect safe inference:

```typescript
// Inferred as safe (no unsafe operations)
function FormatName(first: string, last: string) -> string {
  return first + " " + last
}

// Inferred as unsafe (calls LLM, no catch)
function Extract(text: string) -> Resume {
  client "gpt-4o"
  prompt #"..."#
}

// Inferred as safe (all Errors handled)
function SafeExtract(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"..."#
} catch {
  _ => null  // Handles all Errors
}

// Inferred as safe (even though it can panic)
function ProcessFirst(items: Item[]) -> Item {
  return items[0]  // Can panic, but no Errors
}
```

### Design Philosophy

This design aligns with BAML's "prototype to production" philosophy:

1. **During prototyping**: Use `array[0]`, `assert()`, `todo()` freely. Panics help you catch bugs early.
2. **During hardening**: Add `safe` to functions to ensure all `Error` types are handled.
3. **For production**: Replace panic-prone code with safe accessors (`array.get(0)`) where appropriate.

**The key insight**: `safe` means "handles expected failures," not "cannot fail under any circumstances." Bugs (panics) are a separate concern from error handling.
