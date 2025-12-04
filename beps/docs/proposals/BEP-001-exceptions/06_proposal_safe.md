# Safe Functions: Practical Guide

This document covers the `safe` keyword for strict error handling guarantees.

---

## Using `safe`

### How do I guarantee a function handles all its errors?

Add the `safe` keyword to the function declaration:

```typescript
safe function Extract(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"..."#
} catch {
  e: TimeoutError => null
  e: RefusalError => null
  // Compiler enforces: all Error types must be handled
}
```

Without `safe`, unhandled errors implicitly propagate. With `safe`, the compiler requires exhaustive handling—if you miss an error type, it fails to compile.

### How do I enforce exhaustive error handling at a call site?

Add `safe` before the expression:

```typescript
let user = safe GetUser(id) catch {
  e => null
}
```

Or apply it to a block:

```typescript
let result = safe {
  let x = Compute(data)
  Process(x)
} catch {
  e: CalculationError => 0
  e => -1
}
```

The compiler ensures the attached `catch` block handles all possible errors from the expression or block.

### What does `safe` guarantee?

`safe` guarantees that no `Error` types escape the scope. It does **not** catch `Panic` types.

| With `safe` | Without `safe` |
|:------------|:---------------|
| All Errors must be handled | Unhandled Errors propagate |
| Compiler enforces exhaustiveness | Compiler allows partial handling |
| Panics still propagate | Panics still propagate |

A `safe` function can still crash if a bug occurs (e.g., `assert()` fails, array access panics).

### Does `safe` catch Panics?

No. `safe` only applies to `Error` types. Panics propagate through `safe` functions just like any other function.

```typescript
safe function GetFirst(items: Item[]) -> Item | null {
  return items[0]  // Can panic with IndexOutOfBounds
} catch {
  e => null  // Catches Errors, not Panics
}
```

If `items` is empty, `IndexOutOfBounds` crashes the program despite the `safe` keyword.

### What happens if I add a new error type to a `safe` function?

The compiler fails immediately:

```typescript
safe function Extract(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"..."#
} catch {
  e: TimeoutError => null
  // Error: NetworkError is not handled
}
```

Without `safe`, the new error would silently propagate up the stack. With `safe`, you're forced to handle it.

### Can the compiler infer if a function is safe?

Yes. If a function handles all its errors, the compiler infers it as "semantically safe" even without the keyword.

However, adding `safe` explicitly makes this a **checked contract**. If someone later changes the implementation to introduce an unhandled error, the compiler catches it.

```typescript
// Implicitly safe (compiler infers)
function Extract(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"..."#
} catch {
  e => null  // Handles everything
}

// Explicitly safe (compiler enforces)
safe function Extract(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"..."#
} catch {
  e => null
}
```

---

## Tooling

### Visual Safety Indicators

IDEs display warnings when a function contains unhandled unsafe calls:

```
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

Hovering shows which errors can propagate:

```
⚠ Extract(text) can throw: LLMError, TimeoutError, ParseError

  Add error handling:
    Extract(text) catch { e => defaultResume }
```

### Inline Diagnostics

The compiler flags unsafe calls inside `safe` functions:

```typescript
safe function Process() -> Result {
   let x = Extract(text)
   //      ~~~~~~~~~~~~~ error: unsafe call in safe function
   //      Extract() can throw LLMError, TimeoutError
   //      hint: add `catch { ... }` or use `safe Extract(text) catch { ... }`
   
   let y = Extract(text) catch { e => default }
   //      OK: error handling present
}
```

### Agent Metadata

Generated client code includes safety metadata:

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

Agents can query this before generating code:

```typescript
if (BAML_FUNCTION_METADATA.Extract.isSafe) {
  return `Extract(text)`
} else {
  return `Extract(text) catch { e => null }`
}
```

### CLI Safety Analysis

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
