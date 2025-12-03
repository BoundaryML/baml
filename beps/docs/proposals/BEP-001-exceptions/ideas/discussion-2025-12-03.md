# Discussion: Exception Handling Syntax & Semantics
Date: 2025-12-03

## Context
We are revisiting the syntax for error handling in BAML.
User feedback rejected:

1.  `function = try { ... }` (Breaking change).
2.  `function ... try { ... }` (Syntactically weird).

We need a model that:

1.  Unifies declarative and imperative error handling.
2.  Is familiar (`try/catch` exists).
3.  Doesn't force "double indentation" for the common case (function-level error handling).

## The "Universal Catch" Proposal

Instead of thinking of `try` as a control flow structure, let's think of `catch` as an **operator on blocks**.

### Core Rule
**`catch` can be attached to ANY block.**

1.  **Function Block**:
    ```rust
    function Extract(text) {
       client "gpt4"
       prompt #"..."#
    } catch {
       _ => null
    }
    ```
    *Result*: The catch handles errors from the function body. No extra indentation.

2.  **Imperative Block**:
    ```rust
    let result = {
       let c = Client.new()
       c.run()
    } catch {
       _ => null
    }
    ```
    *Result*: `result` gets the value of the block or the catch.

3.  **Try Block** (Syntactic Sugar):
    ```rust
    let result = try {
       // ...
    } catch {
       _ => null
    }
    ```
    *Theory*: `try { ... }` is identical to `{ ... }`, but it **signals intent** to the reader.

### Why this solves the tension

1.  **"I shouldn't have to learn two ways"**: You don't. You learn **one way**: "Attach `catch` to the thing that might fail."
    - If the "thing" is a function, attach it to the function.
    - If the "thing" is a specific block of code, attach it to that block.

2.  **"Mixing declarative and imperative is confusing"**:
    - You don't *have* to put a `try` block inside your declarative function. You can just attach `catch` to the outside.
    - But if you *want* granular error handling inside, you *can* use `try { ... }` (or just `{ ... }`), and it works the same way.

3.  **"Familiarity"**:
    - We keep `try` as a valid keyword for imperative code where it feels natural.
    - We allow omitting it for function-level declarations where it feels "weird" or causes indentation drift.

### Visualizing the Unification

| Context | Syntax | "Implicit" or "Explicit"? |
| :--- | :--- | :--- |
| **Function Level** | `function F() { ... } catch { ... }` | Implicit Try (Scope = Function Body) |
| **Statement Level** | `let x = try { ... } catch { ... }` | Explicit Try (Scope = Block) |
| **Expression Level** | `let x = { ... } catch { ... }` | Implicit Try (Scope = Block) |

**Key Insight**: `try` is just a "loud" block opener. It's optional semantically but helpful for readability in imperative code.

## Addressing the "Declarative Try" Tension

The user found `try { client ... }` confusing.
With Universal Catch, you avoid this by defaulting to **Function-Level Catch** for LLM functions.

```rust
// ✅ Natural: Catch is part of the function definition
function Extract(text) {
  client "gpt4"
  prompt #"..."#
} catch {
  _ => null
}
```

But if you have complex logic *inside* an imperative function:

```rust
// ✅ Natural: Explicit try for a dangerous subsection
function ComplexLogic() {
  let safe_part = ...
  
  let risky_part = try {
     CallLLM()
  } catch {
     _ => null
  }
  
  return safe_part + risky_part
}
```

This seems to satisfy all constraints:

- No breaking changes.
- No "weird" syntax like `function try`.
- Consistent mental model ("Catch attaches to blocks").
- Solves indentation tax for the main case.

## Questions for User
1.  Does "Universal Catch" (where `try` is just an optional marker for a block) feel consistent to you?
2.  Does this satisfy the "one way to do things" requirement? (The "way" is "attach catch to blocks").

---

# Appendix: Design Rationale & Rejected Alternatives

## The Problem: The "Refactoring Tax"

In AI Engineering, failure is normal, not exceptional. Code often evolves from a "Happy Path" prototype to a "Resilient" production system.

**The Pain Point**: In traditional languages, adding error handling to a function requires a **Structural Refactor**.

1.  **Indentation Tax**: Wrapping code in `try { ... }` forces re-indenting the entire body.
2.  **Hoisting Tax**: Variables defined in the `try` block are scoped to it. To use them later, you must hoist declarations outside.
3.  **Viral Refactor**: Changing a return type to `Result<T>` breaks all callers.

**Goal**: BAML seeks **Additive Resilience**. You should be able to "snap on" error handling without rewriting the happy path.

## Rejected Alternatives

### 1. Standard `try/catch` Statement

**Original Code**:
```rust
function Extract(text) {
  let client = Client.new();
  return client.run(text);
}
```

**Syntax Update (The "Refactoring Tax")**:
```typescript
function Extract(text) {
  // 1. Hoisting Tax: Must declare variable outside
  let client: Client | null = null;
  
  // 2. Indentation Tax: Everything moves right
  try {
    client = Client.new();
  } catch {
    return null;
  }
  
  // 3. Safety Tax: Must assert or check for null
  if (client == null) {
     // What do we do here? We already caught the error?
     // This flow is confusing.
     return null; 
  }
  return client.run(text);
}
```

**Rejected Because**:

- **Indentation Tax**: Forces re-indenting the happy path.
- **Hoisting**: Variable scoping is painful and requires explicit `| null` types and assertions.
- **Declarative Mismatch**: Wrapping declarative `client` definitions in an imperative `try` block feels semantically wrong.

### 2. Result Types (`Result<T, E>`)
```rust
function Extract(text) -> Result<Resume, Error> { ... }
```
**Rejected Because**:

- **Viral**: Changing a return type breaks all callers.
- **Verbosity**: Requires unwrapping at every call site, even for "scripting" use cases.

### 3. Expression-Oriented Try (`let x = try { ... }`)

**The Good (Imperative Code)**:
It solves the hoisting problem beautifully for imperative code.
```rust
function FetchData() -> Data | null {
  // ✅ Clean: No hoisting, 'data' is assigned the result
  let data = try {
     let c = Client.new()
     c.fetch()
  } catch {
     _ => null
  }
  return data
}
```

**The Bad (Declarative Code)**:
It falls apart when wrapping declarative configurations.
```rust
function Extract(text) -> Resume | null {
  // ❌ Confusing: "Try to define a client?"
  // The client definition is static configuration, not an operation to "try".
  let result = try {
    client "openai/gpt-4o"
    prompt #"..."#
  } catch {
    _ => null
  }
  return result
}
```
**Status**: **Accepted** as part of "Universal Catch", but **Rejected** as the *only* way because:

- **Conceptual Mismatch**: Users asked "Why am I wrapping the *definition* of the client in a try block?". It implies the *definition* fails, but really the *execution* (which is implicit in BAML) fails.
- **Indentation**: Still forces indentation for the top-level function case.

### 4. Function-Level Try Modifier (`function ... try`)
```rust
// The return type makes the 'try' look stranded
function Extract(text) -> Resume | null try {
  client "..."
} catch { ... }
```
**Rejected Because**:

- **Syntax**: "Looks weird" (User feedback). The `try` keyword appears *after* the return type but *before* the body.
- **Inconsistency**: `try` usually starts a block, it doesn't modify a function declaration.

### 5. Prefix Try Modifier (`try function ...`)
```rust
try function Extract(text) -> Resume | null {
  client "..."
} catch { ... }
```
**Rejected Because**:

- **Oddity**: "Feels odd" (User feedback).
- **Grammar**: `try` is a verb, `function` is a noun/keyword. `try function` reads like "attempt to define a function", not "define a function that attempts something".

### 6. Assignment-Level Catch (`let x = ... catch ...`)
```rust
let client = Client.new() catch { _ => null }
```
**Status**: **Accepted** (as "Inline Catch"), but insufficient on its own.

- Doesn't handle complex recovery logic that requires multiple statements.
- Doesn't solve the function-level case.

### 7. Breaking Change: `function = try { ... }`
```rust
function Extract(text) -> Resume | null = try { ... }
```
**Rejected Because**:

- **Breaking Change**: Changes the fundamental syntax of function definitions in BAML.
- **Too Radical**: Unnecessary deviation from C-style syntax.

### 8. Wrapper Functions (No Catch on Declarative Blocks)
Force users to wrap declarative functions in a separate imperative function to handle errors.

```rust
// 1. Define the unsafe declarative function
function ExtractUnsafe(text) -> Resume {
  client "gpt4"
  prompt #"..."#
}

// 2. Define a safe wrapper
function Extract(text) -> Resume | null {
  try {
    return ExtractUnsafe(text)
  } catch {
    return null
  }
}
```
**Rejected Because**:

- **Viral Refactor**: You have to rename the original function (breaking all callers, tests, and evals) or name the new one differently.
- **Boilerplate**: Forces creating two functions for every LLM call that needs error handling.
- **Tooling Loss**: We risk losing tooling support (like prompt previews) if the error handling logic is separated from the prompt definition.
- **Irony**: Declarative blocks are the *most likely* to fail (LLMs), so forbidding direct error handling on them is counter-intuitive.

## Selected Approach: Universal Catch

We selected **Universal Catch** because it offers the best compromise:

- **Additive**: `function F() { ... } catch { ... }` allows adding resilience without touching the body.
- **Familiar**: `try { ... }` is supported as syntactic sugar for imperative blocks.
- **Consistent**: The rule is simple—`catch` attaches to *any* block (function, `if`, `for`, or anonymous).
