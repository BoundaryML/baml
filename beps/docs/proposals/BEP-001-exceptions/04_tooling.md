# Tooling Implications

## What We Can Do

### Unreachable Pattern Detection

The compiler detects patterns shadowed by preceding patterns:

```typescript
} catch {
  e => null                       // Catches all Errors
  e: TimeoutError => retry()      // Warning: unreachable pattern
}
```

```
warning: unreachable pattern
  --> src/main.baml:18:3
   |
16 |   e => null
   |   - this pattern matches any error
18 |   e: TimeoutError => retry()
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^ this pattern will never match
```

Also detects patterns for errors a function cannot throw:

```typescript
function FormatName(first: string, last: string) -> string {
  return first + " " + last
} catch {
  e: TimeoutError => null         // Warning: FormatName cannot throw TimeoutError
}
```

### Type Narrowing

Within a handler, the bound variable has the matched type:

```typescript
} catch {
  e: ApiError => {
    log.warn(`API error: ${e.code} - ${e.message}`)  // e is ApiError
    if (e.code == 429) {
      sleep(e.retryAfter)
    }
  }
}
```

### Prompt Preview Preservation

Additive `catch` keeps `client`/`prompt` in the function body, preserving IDE preview features:

```typescript
// ✓ Prompt Preview works
function ExtractResume(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"Extract resume from {{ text }}"#
} catch {
  e => null
}
```

Wrapper-based approaches break this:

```typescript
// ✗ Prompt Preview fails: declarations hidden in internal function
function ExtractResume(text: string) -> Resume | null {
  try { return _ExtractResumeInternal(text) } catch { return null }
}
function _ExtractResumeInternal(text: string) -> Resume {
  client "gpt-4o"
  prompt #"Extract resume from {{ text }}"#
}
```

### Quick Fix: Add Error Handling

The IDE can append a `catch` block with stubs for known error types:

```typescript
// Before
function Extract(text: string) -> Resume {
  client "gpt-4o"
  prompt #"..."#
}

// After quick fix
function Extract(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"..."#
} catch {
  e: TimeoutError => todo("Handle timeout")
  e: ParseError => todo("Handle parse error")
  e => null
}
```

### Autocomplete

Error type suggestions based on the expression's throw signature:

```typescript
} catch {
  e:  // Autocomplete: TimeoutError, NetworkError, ParseError, Panic
}
```

### Linter: Warn on Catching Panic

Warn when `Panic` is caught:

```typescript
} catch {
  p: Panic => DefaultResult()  // Warning: catching Panic hides bugs
  e => DefaultResult()
}
```

Suppress with a directive when catching panics is intentional (e.g., server entry points):

```typescript
// @baml-lint-ignore catch-panic
function ServerMain() {
  RunApp()
} catch {
  p: Panic => { log.fatal("Bug", p); ErrorResponse() }
  e => ErrorResponse()
}
```

### Linter: Warn on `todo()`

`todo()` marks incomplete code. The linter warns on any `todo()` call:

```typescript
} catch {
  e: RateLimitError => todo("Implement retry")  // Warning: incomplete implementation
  e => null
}
```

## What We Cannot Do

### No Mandatory Exhaustiveness Checking

`catch` blocks do not require exhaustiveness. This compiles:

```typescript
function Extract(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"..."#
} catch {
  e: TimeoutError => null
  // ParseError, NetworkError implicitly re-thrown
}
```

The compiler adds implicit re-throws:

```typescript
// Compiler desugaring:
} catch {
  e: TimeoutError => null
  __implicit_panic: Panic => throw __implicit_panic  // Re-throw all Panics
  __implicit_error => throw __implicit_error         // Re-throw unhandled Errors
}
```

**Tradeoff**: Mandatory exhaustiveness would force handling all errors upfront, breaking the prototype-to-production workflow. Implicit re-throw allows gradual hardening.

### No `throws` Declarations

Functions do not declare thrown types (`throws TimeoutError, ParseError`). Inferred throw signatures would be viral—adding a new error in a low-level function cascades through all callers.

### No `finally` Block

Cleanup handled through normal control flow or destructor semantics. May revisit if patterns emerge.

### No Exception Chaining

No automatic cause tracking ("ParseError caused by NetworkError"). Adds runtime overhead; logging at catch point is usually sufficient.
