# Safe/Unsafe Function Coloring

## Problem Statement

In BAML, functions that interact with LLMs, perform I/O, or can throw exceptions are inherently **unsafe** — they can fail at runtime. This creates a challenge for compositional code:

1. **How do we know if a function can throw?** Without explicit marking or inference, a developer (or AI agent) can't tell if calling a function might introduce errors into their code path.

2. **How do we guarantee safety when we need it?** In contexts that require total functions (e.g., returning a non-optional type, or building guaranteed-safe APIs), we need a way to enforce that all errors are handled.

3. **How do we expose safe APIs to agents?** When AI agents are composing BAML code, they need to know: "Can I call this function in a context that doesn't allow errors, or do I need to handle the errors first?"

This proposal introduces a **function coloring system** with three complementary mechanisms:

1. **Automatic inference** of unsafe functions
2. **Call-site safety enforcement** via the `safe` keyword
3. **Declaration-time safety constraints** via the `safe` function modifier

## Background: Why Function Coloring Matters for BAML

BAML is designed for **AI Engineering**, where:

- **LLM calls are probabilistic** — they can fail, timeout, or return malformed data
- **Error handling is core control flow** — not an edge case
- **Agents write code** — they need static guarantees about what can throw
- **Progressive hardening** — code evolves from prototype to production

In this context, we need **compositional safety guarantees**:

```baml
// An agent building a pipeline needs to know:
// "Can I use ExtractResume here, or will it throw?"
function BuildReport(text: string) -> Report {
   let resume = ExtractResume(text)  // ❓ Is this safe?
   return Report { data: resume }
}
```

Without function coloring, the agent (or developer) must:
- Read the implementation of `ExtractResume`
- Trace all function calls recursively
- Check for `client` blocks, I/O, or `throw` statements

This doesn't scale. We need **static, compositional reasoning** about safety.

## Design: Three-Part Mechanism

### 1. Automatic Inference of Unsafe Functions

A function is automatically inferred as **unsafe** (can throw) if it:

- Contains a `throw` statement
- Calls an LLM (has a `client` block)
- Performs I/O operations (`fetch`, disk reads/writes, etc.)
- Calls another unsafe function (unsafety propagates)

**Examples:**

```baml
// Unsafe: calls LLM
function ExtractResume(text: string) -> Resume {
   client "openai/gpt-4o"
   prompt #"Extract resume from: {{ text }}"#
}

// Unsafe: calls unsafe function
function ProcessResume(text: string) -> Resume {
   return ExtractResume(text)  // Propagates unsafety
}

// Unsafe: explicit throw
function ValidateAge(age: int) -> int {
   if (age < 0) {
      throw ValidationError("Age cannot be negative")
   }
   return age
}

// Safe: pure computation
function FormatName(first: string, last: string) -> string {
   return first + " " + last
}
```

**Key Insight:** Unsafety is **viral** — it propagates up the call chain automatically.

### 2. Call-Site Safety Enforcement: `safe` Keyword

The `safe` keyword at a call site **requires** that all possible errors are handled, guaranteeing the expression cannot throw.

**Syntax:**
```baml
safe <expression> catch { <handlers> }
```

**Semantics:**

- The compiler enforces that the expression has a catch block
- The catch block must be **exhaustive** (handle all error types)
- The result is **guaranteed** to be a value (no errors escape)
- `safe` does **not** change the return type — it only provides a compile-time guarantee

**Examples:**

```baml
// ❌ Compile Error: 'safe' requires a catch block
let resume = safe ExtractResume(text)

// ✅ OK: All errors caught with wildcard
let resume = safe ExtractResume(text) catch { 
   _ => defaultResume 
}

// ✅ OK: Exhaustive pattern matching
let resume = safe ExtractResume(text) catch {
   TimeoutError => retryOnce(text)
   ParseError => defaultResume
   _ => defaultResume  // Wildcard catches everything else
}

// ✅ OK: safe on an already-safe expression (redundant but valid)
let name = safe FormatName("John", "Doe")  // No-op since FormatName is safe
```

**Nesting:**

Each `safe` is independent. If you nest them, each level must be independently safe:

```baml
// Both inner and outer must be safe
let result = safe Foo(
   safe Bar() catch { _ => defaultBar }
) catch { _ => defaultFoo }
```

**Where `safe` can be used:**

`safe` works on **expressions only**, not statements:

```baml
// ✅ OK: on function calls
let x = safe GetData() catch { _ => null }

// ✅ OK: on blocks (blocks are expressions)
let result = safe {
   let x = UnsafeOp1()
   let y = UnsafeOp2()
   x + y
} catch { _ => 0 }

// ✅ OK: on inline ternaries
let x = safe (condition ? RiskyTrue() : RiskyFalse()) catch { _ => null }

// ❌ NOT allowed: on statements like for loops
safe for (item in items) {  // Not an expression!
   ProcessUnsafe(item)
}
```

### 3. Declaration-Time Safety Constraint: `safe` Function Modifier

You can declare a function as `safe`, which **requires** that it handle all internal errors and never throw.

**Syntax:**
```baml
safe function FunctionName(...) -> ReturnType {
   // body
} catch {
   // REQUIRED: must handle all errors
}
```

**Semantics:**

- The function **must not** let any errors escape
- All unsafe operations inside must be caught
- The function can be freely called without error handling
- This is **enforced at compile time**

**Examples:**

```baml
// ✅ Valid: safe function with comprehensive error handling
safe function SafeExtract(text: string) -> Resume | null {
   client "openai/gpt-4o"
   prompt #"Extract resume from: {{ text }}"#
} catch {
   _ => null  // All errors handled, returns null
}

// Now callers don't need to handle errors
let resume = SafeExtract(text)  // OK: SafeExtract is guaranteed safe

// ❌ Compile Error: declared 'safe' but has unhandled errors
safe function BadExtract(text: string) -> Resume {
   client "openai/gpt-4o"
   prompt #"Extract: {{ text }}"#
   // ERROR: Missing catch block! Function declared safe but can throw.
}

// ✅ Valid: safe function calling other safe operations
safe function BuildReport(text: string) -> Report | null {
   let resume = SafeExtract(text)  // OK: SafeExtract is safe
   if (resume == null) {
      return null
   }
   return Report { data: resume }
}
// No catch needed because all operations are safe
```

**When to use `safe` functions:**

- **Public API boundaries** — functions exposed to external callers or agents
- **Critical paths** — code that must never crash (e.g., error recovery logic)
- **Composition guarantees** — when you want to guarantee a function is safe for all callers

## Composition Rules

Understanding how safe/unsafe functions compose:

### Rule 1: Unsafe functions propagate unsafety

```baml
function A() -> T {
   // calls LLM
}  // A is unsafe

function B() -> T {
   return A()  // Calls unsafe function
}  // B is unsafe

function C() -> T {
   return B()  // Calls unsafe function
}  // C is unsafe
```

### Rule 2: Safe functions contain all errors

```baml
safe function A() -> T | null {
   // calls LLM
} catch { _ => null }  // A is safe

function B() -> T | null {
   return A()  // Calls safe function
}  // B is safe (no other unsafe operations)
```

### Rule 3: Call-site `safe` doesn't change the caller's safety

```baml
function A() -> T {
   // unsafe operation
}

function B() -> T {
   return safe A() catch { _ => defaultT }  
}  // B is still safe (the call is handled)

function C() -> T {
   return safe A() catch { _ => throwDifferentError() }
}  // C is unsafe (catch block throws)
```

### Rule 4: Mixing safe and unsafe functions

```baml
safe function Process() -> Result | null {
   let x = SafeOp1()  // OK: safe
   let y = SafeOp2()  // OK: safe
   
   // ❌ Compile Error: UnsafeOp is unsafe, must use 'safe' or add catch
   let z = UnsafeOp(x, y)
   
   return Result { x, y, z }
}

// Fix 1: Use 'safe' at call-site
safe function Process() -> Result | null {
   let x = SafeOp1()
   let y = SafeOp2()
   let z = safe UnsafeOp(x, y) catch { _ => null }  // ✅ OK
   return Result { x, y, z }
}

// Fix 2: Add catch to entire function (if not declared safe)
function Process() -> Result | null {
   let x = SafeOp1()
   let y = SafeOp2()
   let z = UnsafeOp(x, y)  // OK: caught below
   return Result { x, y, z }
} catch {
   _ => null
}
```

## Type System Interaction

**Key Principle:** `safe` does **not** change types, only provides guarantees.

```baml
function UnsafeExtract(text: string) -> Resume {
   client "openai/gpt-4o"
   prompt #"Extract: {{ text }}"#
}

// Both have the same type: Resume | null
let a = UnsafeExtract(text) catch { _ => null }
let b = safe UnsafeExtract(text) catch { _ => null }

// The difference:
// - 'a' might still throw if catch doesn't handle all errors
// - 'b' is GUARANTEED not to throw (compile-time enforced)
```

The value of `safe` is the **compile-time guarantee**, not a type change.

## Alternative: Safe<T> as a Phantom Type

An alternative (or complementary) approach is to make `safe` visible in the type system using a **phantom type** `Safe<T>`.

### What is Safe<T>?

`Safe<T>` is a **compile-time marker** that gets **completely erased at runtime**. It exists only to track safety in the type system.

**Key Properties:**

1. **Runtime Erasure:** `Safe<T>` and `T` are **identical** at runtime
2. **Type-level Only:** `Safe<T>` can only appear in **function signatures**, not in value types
3. **Colorless Subtyping:** `Safe<T>` is a **subtype** of `T` (can be used where `T` is expected)

### Valid Uses of Safe<T>

```baml
// ✅ 1. Function return types
function SafeExtract() -> Safe<Resume> { ... }

// ✅ 2. Lambda/callable types
type SafeExtractor = (string) -> Safe<Resume>
type UnsafeExtractor = (string) -> Resume

// ✅ 3. Higher-order function parameters (function types)
function MapSafe(items: string[], fn: (string) -> Safe<Resume>) -> Resume[] {
   // fn is guaranteed not to throw
}

// ✅ 4. Function type fields in classes
class Pipeline {
   extractor: (string) -> Safe<Resume>
}
```

### Invalid Uses

```baml
// ❌ Variables can't be Safe<T>
let x: Safe<Resume> = SafeExtract()  // Error: use Resume instead

// ❌ Regular parameters can't be Safe<T>
function Process(r: Safe<Resume>) -> void { ... }  // Error
```

**Why?** Because `Safe<T>` is about **how the value was produced** (safely), not **what the value is**. Once you have a value, it's just `T`.

### The `safe` Keyword as Syntactic Sugar

Similar to how `async function` automatically wraps the return type in `Promise<T>` without writing `Promise`:

```typescript
// TypeScript async example
async function fetchData() -> Data {  // Actually returns Promise<Data>
   ...
}
```

In BAML, `safe function` would automatically wrap the return type in `Safe<T>`:

```baml
// These are equivalent:
safe function Extract() -> Resume { ... }
function Extract() -> Safe<Resume> { ... }

// Both mean: returns Resume and is guaranteed not to throw
```

**Explicit writing:**

```baml
// You can explicitly write Safe<T> if you prefer
function Extract() -> Safe<Resume> {
   client "openai/gpt-4o"
   prompt #"..."#
} catch { _ => default }

// Or use the safe keyword for brevity
safe function Extract() -> Resume {
   client "openai/gpt-4o"
   prompt #"..."#
} catch { _ => default }
```

### Subtyping Rules

`Safe<T>` is **colorless** — it can be used anywhere `T` is expected:

```baml
// Function that might throw
function UnsafeExtract() -> Resume { ... }

// Function guaranteed not to throw
function SafeExtract() -> Safe<Resume> { ... }

// Function accepting any Resume
function Process(r: Resume) -> void { ... }

// ✅ OK: Resume works with Resume
let r1 = UnsafeExtract()
Process(r1)

// ✅ OK: Safe<Resume> works with Resume (subtyping)
let r2 = SafeExtract()
Process(r2)  // Safe<Resume> is compatible with Resume
```

But the reverse is **not** true:

```baml
// Function requiring safe Resume
function ProcessSafe(fn: (string) -> Safe<Resume>) -> void { ... }

safe function SafeExt() -> Resume { ... }
function UnsafeExt() -> Resume { ... }

// ✅ OK: Safe<Resume> -> Safe<Resume>
ProcessSafe(SafeExt)

// ❌ Error: Resume is not compatible with Safe<Resume>
ProcessSafe(UnsafeExt)  // Type error!
```

### Powerful Composition Patterns

#### Pattern 1: Safe vs Unsafe Callbacks

```baml
// Accepts any extractor (safe or unsafe)
function MapAny(items: string[], fn: (string) -> Resume) -> Resume[] {
   let results = []
   for (item in items) {
      results.append(fn(item))  // Might throw!
   } catch {
      e => log("Batch processing failed", e)
   }
   return results
}

// ONLY accepts safe extractors
function MapSafe(items: string[], fn: (string) -> Safe<Resume>) -> Resume[] {
   let results = []
   for (item in items) {
      results.append(fn(item))  // Guaranteed not to throw
   }
   return results  // No catch block needed!
}

// Define extractors
let unsafe_fn = (text: string) -> Resume { ExtractUnsafe(text) }

safe function safe_fn(text: string) -> Resume { 
   ExtractUnsafe(text) 
} catch { _ => default }

// Usage
MapAny(items, unsafe_fn)   // ✅ OK
MapAny(items, safe_fn)     // ✅ OK: Safe<Resume> is compatible with Resume
MapSafe(items, unsafe_fn)  // ❌ Error: needs (string) -> Safe<Resume>
MapSafe(items, safe_fn)    // ✅ OK
```

#### Pattern 2: Pipeline with Safe Components

```baml
class Pipeline {
   // Store only safe extractors
   extractors: ((string) -> Safe<Resume>)[]
   
   function addExtractor(fn: (string) -> Safe<Resume>) {
      this.extractors.append(fn)
   }
   
   function process(text: string) -> Resume[] {
      // No error handling needed - all extractors are safe!
      return this.extractors.map(fn => fn(text))
   }
}

// Adding extractors
let pipeline = Pipeline { extractors: [] }

safe function SafeExt(text: string) -> Resume { ... }
function UnsafeExt(text: string) -> Resume { ... }

pipeline.addExtractor(SafeExt)     // ✅ OK
pipeline.addExtractor(UnsafeExt)   // ❌ Error: unsafe function not allowed
```

#### Pattern 3: Optional Safe Variants

```baml
// A service that can provide either safe or unsafe extractors
class ExtractorService {
   function getExtractor(robust: bool) -> (string) -> Resume | Safe<Resume> {
      if (robust) {
         return (text: string) -> Safe<Resume> {
            Extract(text)
         } catch { _ => default }
      } else {
         return (text: string) -> Resume { Extract(text) }
      }
   }
}
```

#### Pattern 4: Retry Logic with Safety Guarantees

```baml
// Takes an unsafe function and makes it safe by retrying
function MakeSafe(
   fn: (string) -> Resume,  // Accepts unsafe function
   retries: int
) -> (string) -> Safe<Resume> {  // Returns safe function
   
   return (text: string) -> Safe<Resume> {
      let result = null
      
      for (_ in range(retries)) {
         result = fn(text)
      } catch {
         _ => {}
      }

      return result ?? defaultResume
   }
}

// Usage
function UnsafeExtract(text: string) -> Resume { ... }

let safeExtract = MakeSafe(UnsafeExtract, retries: 3)
// safeExtract has type: (string) -> Safe<Resume>

// Can be used in contexts requiring safe functions
MapSafe(items, safeExtract)  // ✅ OK
```

### Comparison: Keyword vs Type-Based

| Aspect | `safe function` keyword | `-> Safe<T>` type |
|--------|------------------------|-------------------|
| **Brevity** | More concise | More explicit |
| **Visibility** | Safety is a modifier | Safety is in the type |
| **Composition** | Uses subtyping rules | Uses subtyping rules |
| **Higher-order** | Requires special handling | Natural with function types |
| **Runtime cost** | Zero | Zero (phantom type) |
| **Familiarity** | Similar to Swift's `throws` | Similar to Rust's `Result` |

**The Best of Both Worlds:**

Use the `safe` keyword as **syntactic sugar** for `Safe<T>`:

```baml
// Write this (concise):
safe function Extract() -> Resume { ... }

// Compiler sees this (explicit):
function Extract() -> Safe<Resume> { ... }

// Developers think in terms of 'safe function'
// Type system reasons about Safe<T>
// Higher-order functions get safety for free
```

This parallels TypeScript's `async/await`:
- Write: `async function fetch() -> Data`
- Type: `function fetch() -> Promise<Data>`

### Benefits for BAML

1. **Natural Higher-Order Functions:** Function types automatically carry safety information
2. **Type-Driven Development:** Types tell you exactly what's safe
3. **Zero Runtime Cost:** `Safe<T>` erases completely
4. **Gradual Adoption:** Can start with unsafe code and progressively add `safe` markers
5. **Agent-Friendly:** AI agents can read function types to determine safety

## Why This Matters for BAML

### 1. Agent-Callable APIs

When AI agents are composing BAML code, they need to know what's safe to call:

```baml
// Agent sees this in the API
safe function GetUserData(id: string) -> User | null

// Agent knows: "I can call this without error handling"
let user = GetUserData(id)
```

Without the `safe` marker, the agent would need to:
- Inspect the implementation
- Trace all function calls
- Guess if error handling is needed

### 2. Progressive Hardening

You can start with unsafe prototypes and progressively make them safe:

```baml
// Prototype: quick and dirty
function ExtractResume(text: string) -> Resume {
   client "openai/gpt-4o"
   prompt #"Extract: {{ text }}"#
}

// Later: harden for production
safe function ExtractResume(text: string) -> Resume | null {
   client "openai/gpt-4o"
   prompt #"Extract: {{ text }}"#
} catch {
   e: TimeoutError => retry(text)
   e: ParseError => null
   _ => null
}
```

### 3. Compositional Safety

Build safe pipelines from safe components:

```baml
safe function ProcessBatch(texts: string[]) -> Resume[] {
   let results = []
   for (text in texts) {
      let resume = safe ExtractResume(text) catch { _ => null }
      if (resume != null) {
         results.append(resume)
      }
   }
   return results
}
// Guaranteed to never crash, even if individual extractions fail
```

## Open Questions

1. **Safe function with explicit throws in catch**
   ```baml
   safe function Foo() -> T {
      UnsafeOp()
   } catch {
      e => throw WrapperError(e)  // ❓ Allowed? Or compile error?
   }
   ```
   **Proposed:** Compile error. Safe functions cannot throw, even in catch blocks.

2. **Inference vs. explicit marking for generated code**
     - Should the compiler generate metadata indicating which functions are safe?
     - This would help tools (IDEs, agents) quickly determine safety without re-analysis.

3. **Standard library and built-ins**
     - Which built-in functions are safe vs. unsafe?
     - `fetch` is clearly unsafe, but what about `string.parse_int()`?
     - Should we have a way to mark external/FFI functions as safe/unsafe?

4. **Exhaustiveness checking**
     - How do we verify a catch block is exhaustive?
     - Do we need a type hierarchy of errors?
     - Or is a wildcard `_` always required for exhaustiveness?

5. **Interaction with async/await** (if BAML adds async)
     - Does `safe async function` make sense?
     - How do async errors propagate?

6. **Gradual typing**
     - Can we have a "lenient mode" where safe/unsafe is optional?
     - Or should this be enforced from day one?

## Comparison to Other Languages

### Rust: `Result<T, E>` and `?` operator

- **Similar:** Both track fallibility in the type system
- **Different:** Rust changes return types (`Result`), BAML uses effect tracking (`safe`)

### Swift: `throws` keyword

- **Similar:** Functions marked with `throws`, must be called with `try`
- **Different:** Swift requires `try` at call-site, BAML uses `safe` for guarantees

### Go: Error return values

- **Similar:** Explicit error handling
- **Different:** Go doesn't track which functions can error at compile-time

### Java: Checked exceptions

- **Similar:** Compiler enforces handling of declared exceptions
- **Different:** Java's checked exceptions are controversial (viral, verbose), BAML's `safe` is opt-in

**BAML's Advantage:** The combination of inference + call-site enforcement + declaration constraints provides flexibility without the "viral refactoring" problem of checked exceptions.

## Tooling Support

To make the safe/unsafe system effective, we need comprehensive tooling support across three key areas:

### 1. Visual Indicators (IDE/Editor)

Visual feedback is critical for developers to quickly understand safety implications.

#### Gutter Icons

```baml
  1 │ // Simple, minimal gutter indicators
  2 │ ● safe function SafeExtract(text: string) -> Resume | null {
  3 │ ⚠    client "openai/gpt-4o"
  4 │      prompt #"Extract: {{ text }}"#
  5 │    } catch { _ => null }
  6 │
  7 │ ⚠ function Extract(text: string) -> Resume {
  8 │ ⚠    client "openai/gpt-4o"
  9 │      prompt #"Extract: {{ text }}"#
 10 │    }
 11 │
 12 │ ● function FormatName(first: string, last: string) -> string {
 13 │      return first + " " + last
 14 │    }
```

**Icon meanings:**
- `●` (green) = Safe - cannot throw
- `⚠` (yellow/orange) = Unsafe - can throw
- `◌` (gray) = Unchecked - safety checking disabled

#### Inline Diagnostics

```baml
safe function Process() -> Result {
   let x = Extract(text)
   //      ~~~~~~~~~~~~~ ⚠ Unsafe call in safe function
   //      Extract() can throw but Process() is safe
   
   let y = Extract(text) catch { _ => default }
   //      ✓ No warning - error handling present
}
```

#### Hover Tooltips

Hovering over an unsafe function call shows:

```
╔═══════════════════════════════════════════════════════════╗
║ function Extract(text: string) -> Resume                  ║
╟───────────────────────────────────────────────────────────╢
║ ⚠ Unsafe Function                                         ║
║                                                           ║
║ This function can throw:                                  ║
║   • LLMError                                              ║
║   • TimeoutError                                          ║
║   • ParseError                                            ║
║                                                           ║
║ 💡 Add error handling:                                    ║
║    Extract(text) catch { _ => default }                   ║
╚═══════════════════════════════════════════════════════════╝
```

Hovering over a safe function shows:

```
╔═══════════════════════════════════════════════════════════╗
║ safe function SafeExtract(text: string) -> Resume | null  ║
╟───────────────────────────────────────────────────────────╢
║ ✓ Safe Function                                           ║
║                                                           ║
║ This function never throws. Returns null on error.        ║
╚═══════════════════════════════════════════════════════════╝
```

#### Code Lens (Quick Actions)

```baml
function Process() -> Result {
   let x = Extract(text)
   //      └─ 💡 Add error handling | 💡 Use SafeExtract instead
}
```

Clicking "Add error handling" generates:

```baml
let x = Extract(text) catch {
   _ => |  // Cursor placed here
}
```

#### Status Bar

Bottom of IDE shows current file's safety stats:

```
┌────────────────────────────────────────────────────────────┐
│ BAML  ● 3 safe  ⚠ 2 unsafe  ◌ 0 unchecked                 │
└────────────────────────────────────────────────────────────┘
```

### 2. Safety Metadata

Generated client code includes comprehensive safety metadata for runtime inspection and tooling.

#### Metadata Structure

```typescript
// baml_client/types.ts

export interface FunctionMetadata {
  name: string;
  isSafe: boolean;
  canThrow: string[];
  parameters: ParameterMetadata[];
  returnType: string;
}

export const BAML_FUNCTION_METADATA = {
  Extract: {
    name: "Extract",
    isSafe: false,
    canThrow: ["LLMError", "TimeoutError", "ParseError"],
    parameters: [
      { name: "text", type: "string", optional: false }
    ],
    returnType: "Resume"
  },
  
  SafeExtract: {
    name: "SafeExtract",
    isSafe: true,
    canThrow: [],
    parameters: [
      { name: "text", type: "string", optional: false }
    ],
    returnType: "Resume | null"
  }
} as const;
```

#### Runtime API

```typescript
import { BAML_FUNCTION_METADATA } from './baml_client/types';

// Check if function is safe
if (BAML_FUNCTION_METADATA.Extract.isSafe) {
  const result = await baml.Extract(text);
} else {
  // Handle errors
  try {
    const result = await baml.Extract(text);
  } catch (e) {
    console.log('Expected errors:', BAML_FUNCTION_METADATA.Extract.canThrow);
  }
}

// Get all safe functions
const safeFunctions = Object.entries(BAML_FUNCTION_METADATA)
  .filter(([_, meta]) => meta.isSafe)
  .map(([name, _]) => name);
```

#### JSON Metadata File

```json
// baml_client/metadata.json
{
  "version": "0.1.0",
  "generatedAt": "2024-12-03T10:30:00Z",
  "functions": {
    "Extract": {
      "name": "Extract",
      "safety": {
        "isSafe": false,
        "canThrow": ["LLMError", "TimeoutError", "ParseError"]
      },
      "source": {
        "file": "functions.baml",
        "line": 7
      }
    }
  },
  "statistics": {
    "totalFunctions": 3,
    "safeFunctions": 2,
    "unsafeFunctions": 1,
    "safetyPercentage": 66.7
  }
}
```

#### Language-Specific Helpers

**Python:**
```python
# baml_client/metadata.py

BAML_FUNCTION_METADATA = {
    "Extract": {
        "name": "Extract",
        "is_safe": False,
        "can_throw": ["LLMError", "TimeoutError", "ParseError"],
        "return_type": "Resume"
    }
}

def is_safe(function_name: str) -> bool:
    """Check if a BAML function is safe."""
    return BAML_FUNCTION_METADATA[function_name]["is_safe"]

def get_safe_functions() -> List[str]:
    """Get all safe BAML functions."""
    return [
        name for name, meta in BAML_FUNCTION_METADATA.items()
        if meta["is_safe"]
    ]
```

#### Generated Documentation

Functions include safety information in documentation:

```typescript
export class BamlClient {
  /**
   * Extract resume information from text.
   * 
   * @safety UNSAFE
   * @throws {LLMError} When the LLM call fails
   * @throws {TimeoutError} When the request times out
   * @throws {ParseError} When the response cannot be parsed
   * 
   * @see {@link SafeExtract} for a safe alternative
   */
  async Extract(text: string): Promise<Resume> { }

  /**
   * Safely extract resume information from text.
   * 
   * @safety SAFE - Never throws
   * @returns Resume object or null on error
   */
  async SafeExtract(text: string): Promise<Resume | null> { }
}
```

### 3. CLI Tools

Command-line tools for analyzing and tracking safety across the codebase.

#### `baml safety check`

Check for safety issues:

```bash
$ baml safety check

Checking safety in BAML project...

✓ baml_src/functions.baml
  ● SafeExtract (line 2)
  ⚠ Extract (line 7)
  ● FormatName (line 15)

⚠ baml_src/main.baml
  ● SafeAPI (line 10)
  ⚠ Process (line 45) - 1 issue
     Line 46: Unsafe call to Extract() in safe function
     
Summary:
  Total functions: 5
  ● Safe: 3 (60%)
  ⚠ Unsafe: 1 (20%)
  ◌ Unchecked: 1 (20%)
  
  Issues found: 1 error

❌ Safety check failed
```

**Exit codes:**
- `0` = No issues
- `1` = Errors found
- `2` = Warnings only (if `--strict`)

#### `baml safety stats`

Show safety statistics:

```bash
$ baml safety stats

BAML Safety Statistics
══════════════════════════════════════════════════════════

Project: my-baml-project
Files analyzed: 8

Function Safety
──────────────────────────────────────────────────────────
  Total functions: 25
  ● Safe: 18 (72%)
  ⚠ Unsafe: 6 (24%)
  ◌ Unchecked: 1 (4%)

Safety by File
──────────────────────────────────────────────────────────
  functions.baml:     ●●●⚠⚠ (60% safe)
  main.baml:          ●●●● (100% safe)
  extractors.baml:    ●●⚠⚠⚠ (40% safe)

Top Unsafe Functions
──────────────────────────────────────────────────────────
  1. Extract (functions.baml:7)
     Called by: Process, BatchProcess, SafeAPI
     
  2. FetchData (api.baml:15)
     Called by: GetUserData, GetAllData

Error Types
──────────────────────────────────────────────────────────
  LLMError: 4 functions
  TimeoutError: 3 functions
  ParseError: 3 functions
  NetworkError: 2 functions
```

#### `baml safety graph`

Visualize function call graph with safety information:

```bash
$ baml safety graph --function SafeAPI

SafeAPI (safe)
├─● SafeExtract (safe)
│  └─⚠ Extract (unsafe) [caught]
├─● FormatName (safe)
└─⚠ Process (unsafe) [caught]
   └─⚠ FetchData (unsafe) [caught]

Legend:
  ● = Safe function
  ⚠ = Unsafe function
  [caught] = Error handling present
```

**Output formats:**
- ASCII (default)
- DOT/Graphviz: `baml safety graph --format dot | dot -Tpng > graph.png`
- Mermaid: `baml safety graph --format mermaid`
- JSON: `baml safety graph --format json`

#### `baml safety trace`

Trace error propagation paths:

```bash
$ baml safety trace Extract

Analyzing error propagation for Extract...

Extract (functions.baml:7)
  ⚠ Can throw: LLMError, TimeoutError, ParseError
  
  Called by:
    1. Process (main.baml:46) [⚠ unsafe, propagates]
       └─ SafeAPI (main.baml:10) [● safe, caught]
       
    2. BatchProcess (batch.baml:15) [⚠ unsafe, propagates]
       └─ RunBatch (batch.baml:30) [● safe, caught]
       
    3. SafeExtract (functions.baml:2) [● safe, caught]

Error paths to uncaught contexts:
  ✗ Extract → Process → (throws)
  ✗ Extract → BatchProcess → (throws)
  ✓ All other paths are handled

Recommendation:
  Add error handling in Process and BatchProcess
```

#### Configuration

```toml
# baml.toml

[safety]
# Strictness level: "lenient" | "default" | "strict"
mode = "default"

# Files to exclude from checking
exclude = [
  "**/vendor/**",
  "**/legacy/**"
]

# Functions to ignore
ignore_functions = ["TemporaryHack"]

[safety.cli]
# Default output style: "color" | "plain" | "json"
output = "color"

# Show recommendations
show_recommendations = true
```

## Future Work: Lambdas and Callable Types

This section describes the design for how safety interacts with lambdas and function types—features that may be added to BAML in the future.

### Function Type Safety: Keyword-Primary Approach

Use the `safe` keyword as a **function type modifier**, not a return type wrapper.

**Syntax:**
```baml
// Function type declarations
type SafeExtractor = safe (string) -> Resume
type UnsafeExtractor = (string) -> Resume

// Class fields
class Pipeline {
   extractor: safe (string) -> Resume
   validator: safe (Resume) -> bool
}

// Higher-order function parameters
function MapSafe(items: string[], fn: safe (string) -> Resume) -> Resume[] {
   // fn is guaranteed not to throw
}
```

**Why keyword over `Safe<T>` wrapper:**
- Safety is a property of **the function**, not the return value
- More ergonomic: `safe (string) -> Resume` vs `(string) -> Safe<Resume>`
- Consistent with `safe function` declaration syntax
- No confusion about `Safe<T>` appearing in value types

### Lambda Safety Rules

#### Rule 1: Lambda Safety is Inferred from Body

```baml
let fn = (text) { text.upper() }
// Inferred: safe (string) -> string

let fn = (text) { Extract(text) }
// Inferred: (string) -> Resume (unsafe)

let fn = (text) { Extract(text) } catch { _ => default }
// Inferred: safe (string) -> Resume (errors handled)
```

#### Rule 2: Explicit `safe` Requires Catch or Safe Body

```baml
// ❌ Error: safe lambda with unsafe body and no catch
let fn = safe (text) { Extract(text) }

// ✅ OK: safe lambda with catch
let fn = safe (text) { Extract(text) } catch { _ => default }

// ✅ OK: safe lambda with safe body (no catch needed)
let fn = safe (text) { return default }
```

#### Rule 3: Context Can Accept Safe Body Without `safe` Keyword

```baml
function MapSafe(fn: safe (string) -> Resume) { ... }

// ✅ OK: body is provably safe, no 'safe' keyword needed
MapSafe((text) { return default })

// ❌ Error: body is unsafe, must use 'safe' keyword with catch
MapSafe((text) { Extract(text) })

// ✅ OK: explicit safe with catch
MapSafe(safe (text) { Extract(text) } catch { _ => default })
```

#### Rule 4: Safe Lambdas Are Subtypes of Unsafe

```baml
let unsafeVar: (string) -> T = safe (s) { ... }  // ✅ OK
let safeVar: safe (string) -> T = (s) { ... }    // ❌ Error
```

**Subtyping Rule:**
```
safe (A) -> T  <:  (A) -> T
```

This means:
- Safe functions can be used where unsafe ones are expected
- Unsafe functions cannot be used where safe ones are expected

### Higher-Order Function Patterns

#### Pattern 1: Safe vs Unsafe Callbacks

```baml
// Accepts any extractor (safe or unsafe)
function MapAny(items: string[], fn: (string) -> Resume) -> Resume[] {
   let results = []
   for (item in items) {
      results.append(fn(item))  // Might throw!
   } catch {
      e => log("Batch processing failed", e)
   }
   return results
}

// ONLY accepts safe extractors
function MapSafe(items: string[], fn: safe (string) -> Resume) -> Resume[] {
   let results = []
   for (item in items) {
      results.append(fn(item))  // Guaranteed not to throw
   }
   return results  // No catch block needed!
}

// Usage
let unsafeFn: (string) -> Resume = (text) { Extract(text) }
let safeFn: safe (string) -> Resume = safe (text) {
   Extract(text)
} catch { _ => default }

MapAny(items, unsafeFn)   // ✅ OK
MapAny(items, safeFn)     // ✅ OK: safe <: unsafe
MapSafe(items, unsafeFn)  // ❌ Error: needs safe function
MapSafe(items, safeFn)    // ✅ OK
```

#### Pattern 2: Pipeline with Safe Components

```baml
class Pipeline {
   // Store only safe extractors
   extractors: (safe (string) -> Resume)[]
   
   function addExtractor(fn: safe (string) -> Resume) {
      this.extractors.append(fn)
   }
   
   function process(text: string) -> Resume[] {
      // No error handling needed - all extractors are safe!
      return this.extractors.map(fn => fn(text))
   }
}
```

#### Pattern 3: Function Factory

```baml
// Returns a safe lambda
function MakeSafeExtractor(default: Resume) -> safe (string) -> Resume {
   return safe (text) {
      Extract(text)
   } catch { _ => default }
}

// Returns an unsafe lambda
function MakeUnsafeExtractor() -> (string) -> Resume {
   return (text) {
      Extract(text)  // Can throw
   }
}
```

#### Pattern 4: Retry Logic with Safety Guarantees

```baml
// Takes an unsafe function and makes it safe by retrying
function MakeSafe(
   fn: (string) -> Resume,
   retries: int
) -> safe (string) -> Resume {
   
   return safe (text) {
      let result = null
      
      for (_ in range(retries)) {
         result = fn(text)
      } catch {
         _ => {}
      }

      return result ?? defaultResume
   }
}

// Usage
function UnsafeExtract(text: string) -> Resume { ... }

let safeExtract = MakeSafe(UnsafeExtract, retries: 3)
// safeExtract has type: safe (string) -> Resume

// Can be used in contexts requiring safe functions
MapSafe(items, safeExtract)  // ✅ OK
```

### Closures and Captured State

```baml
// Lambda captures unsafe function
function UnsafeExtract(text: string) -> Resume { ... }

let fn = (text) {
   return UnsafeExtract(text)  // Calls captured unsafe function
}
// Inferred: (string) -> Resume (unsafe)

// Lambda captures safe function
safe function SafeExtract(text: string) -> Resume { ... }

let fn = (text) {
   return SafeExtract(text)  // Calls captured safe function
}
// Inferred: safe (string) -> Resume (safe)

// Lambda captures variables (not functions)
function BuildExtractor(default: Resume) -> safe (string) -> Resume {
   return safe (text) {
      Extract(text)
   } catch { _ => default }  // Uses captured variable
}
// Capturing pure values doesn't affect safety
```

### Design Rationale

**Why keyword-primary over type-based:**
- Consistent syntax everywhere: `safe` prefix
- No conceptual confusion about `Safe<T>` as a value type
- Natural for function declarations and lambda expressions
- Aligns with how developers think: "this function is safe"

**Implementation note:** This design decision resolves the tension between:
- Wanting consistent syntax for function types across contexts (declarations, fields, parameters)
- Avoiding weird wrapper types like `Safe<T>` that look like value types but only apply to functions
- Making safety a first-class property of callable entities

## Summary

This proposal introduces a three-part function coloring system:

1. **Inference** — Automatically detect unsafe functions
2. **Call-site `safe`** — Guarantee an expression cannot throw
3. **Declaration `safe`** — Require a function to handle all errors

Together, these mechanisms provide:

- **Static reasoning** about which functions can fail
- **Compositional safety** for building robust pipelines
- **Agent-friendly APIs** with clear safety guarantees
- **Progressive hardening** from prototype to production

Comprehensive tooling support makes safety visible and actionable:

- **Visual indicators** in IDEs for immediate feedback
- **Safety metadata** in generated code for runtime inspection
- **CLI tools** for analysis and tracking

This aligns with BAML's design philosophy: make error handling **additive**, **compositional**, and **agent-friendly**.
