# The Story of BAML Error Handling

BAML's error handling is designed around a simple truth: **when you introduce an LLM into your code, you introduce probabilistic failure at every step.**

This document tells the story of how we arrived at **Universal Catch**—our solution for handling errors in BAML.

## 1. The Reality of AI Engineering

In traditional software engineering, exceptions represent edge cases. Parsing a well-formed JSON document succeeds deterministically. Network operations may timeout, but this is infrequent enough to be handled as an exceptional case.

LLM-based systems operate under different constraints. When you invoke an LLM:

*   The model may refuse the request due to content policies.
*   The response may be structurally valid but semantically incorrect.
*   Requests may timeout due to model load or prompt complexity.
*   The model may return valid JSON that fails to satisfy the intended semantic contract.

Error handling in AI systems is not exceptional—it is a routine part of control flow. Production systems regularly retry requests, switch between models, or fall back to heuristics.

### The Proposal: Universal Catch

BAML's error handling is **additive**. You shouldn't have to rewrite your happy path to add resilience—you should be able to "snap on" error handling.

**Universal Catch** is simple: `catch` is an operator that can attach to **any block**—functions, loops, conditionals, or block expressions.

```typescript
// Attach to a function
function ExtractResume(text: string) -> Resume | null {
   client "gpt-4o"
   prompt #"Extract resume from: {{ text }}"#
} catch {
   _: TimeoutError => null
}

// Attach to a block expression
let result = {
   let data = FetchData()
   ProcessData(data)
} catch {
   e => { log(e); null }
}

// Attach to an expression (inline catch)
let user = GetUser(id) catch { _ => null }
```

The rest of this document explains *why* we made this choice by showing you the alternatives we considered—and why we moved on from each one.

---

## 2. Design Evolution (Alternatives We Considered)

To evaluate each approach, we'll use two examples:

### The Base Cases

**1. Declarative (LLM Function)**

```typescript
function ExtractResume(text: string) -> Resume {
  client "gpt-4o"
  prompt #"Extract resume from {{ text }}"#
}
```

**2. Imperative (Batch Processing)**

```typescript
function ProcessBatch(urls: string[]) -> Resume[] {
  // If this fails, we still want to process resumes!
  let aggregator = MetricsAggregator.new() 
  let results = []
  
  for (url in urls) {
    let resume = ExtractResume(url)
    aggregator.record(resume)
    results.append(resume)
  }
  return results
}
```

**The Goal**: Handle failures gracefully. For `ExtractResume`, return `null` on timeout. For `ProcessBatch`, handle `MetricsAggregator.new()` failing without stopping the processing.

---

### Attempt 1: The Classic `try/catch` Statement

The traditional approach: wrap risky code in a `try` block.

#### Imperative Code

**The Hoisting Tax** is brutal here. To handle `aggregator` failing, you must:

1.  **Hoist the declaration** outside the `try` block.
2.  Add `| null` to the type.
3.  Check for `null` everywhere you use it.

```typescript
function ProcessBatch(urls: string[]) -> Resume[] {
  // 1. Hoisting Tax: Declare variable with nullable type
  let aggregator: MetricsAggregator | null = null
  
  // 2. Indentation Tax: Wrap initialization
  try {
    aggregator = MetricsAggregator.new()
  } catch {
    // Just log and continue
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
```

The happy path is now polluted with error handling logic. The `if (aggregator != null)` check appears far from where the error was caught, making the flow hard to follow.

#### Declarative Code

Wrapping a declarative client definition in `try` feels semantically wrong:

```typescript
function ExtractResume(text: string) -> Resume | null {
  try {
    // ❌ Confusing: Are we "trying" to define the client?
    // Or trying to call it?
    client "gpt-4o"
    prompt #"Extract resume from {{ text }}"#
  } catch {
    _: TimeoutError => null
  }
}
```

The `client` definition is **static configuration**, not an operation. Wrapping it in `try` implies the *definition* might fail, when really it's the *execution* (implicit in BAML) that fails.

#### Why We Moved On

*   **Indentation Tax**: Forces structural changes to the happy path.
*   **Hoisting Tax**: Variable scoping becomes painful, requiring nullable types and assertions.
*   **Declarative Mismatch**: Wrapping configuration in imperative control flow is confusing.

---

### Attempt 2: Result Types (`Result<T, E>`)

The functional programming approach: make errors part of the return type.

#### Imperative Code

```typescript
function ProcessBatch(urls: string[]) -> Resume[] {
  // MetricsAggregator.new() now returns Result<MetricsAggregator, Error>
  let aggregator_result = MetricsAggregator.new()
  
  // Must explicitly unwrap or match
  let aggregator = match (aggregator_result) {
    Ok(agg) => agg
    Err(e) => {
      log.warn("Failed to initialize", e)
      null  // Still need nullable type!
    }
  }
  
  let results = []
  
  for (url in urls) {
    let resume = ExtractResume(url)  // Also returns Result now!
    
    // Unwrap everywhere
    if (aggregator != null) {
      aggregator.record(resume)
    }
    
    results.append(resume)
  }
  return results
}
```

Every call site must now handle the `Result`. This gets very verbose, very fast.

#### Declarative Code

```typescript
// Changing the return type breaks all callers
function ExtractResume(text: string) -> Result<Resume, Error> {
  client "gpt-4o"
  prompt #"..."#
}

// Now the caller must unwrap
function ProcessUser(text: string) -> User {
  let resume_result = ExtractResume(text)
  let resume = match (resume_result) {
    Ok(r) => r
    Err(e) => Resume.default()
  }
  // ...
}
```

#### Why We Moved On

*   **Viral Complexity**: Changing one function's return type forces changes up the entire call stack.
*   **Verbosity**: Even "scripting" use cases require explicit unwrapping.
*   **Ergonomics**: Works great for Rust's systems programming domain, but too heavy for AI prototyping workflows.

---

### Attempt 3: Expression-Oriented Try (`let x = try { ... }`)

Make `try` an expression that can be assigned to a variable.

#### Imperative Code

This approach **shines** for imperative code:

```typescript
function ProcessBatch(urls: string[]) -> Resume[] {
  // ✅ Beautiful: No hoisting, clean assignment
  let aggregator = try {
    MetricsAggregator.new()
  } catch {
    e => {
      log.warn("Failed to initialize", e)
      null
    }
  }
  
  let results = []
  
  for (url in urls) {
    let resume = ExtractResume(url)
    
    if (aggregator != null) {
      aggregator.record(resume)
    }
    
    results.append(resume)
  }
  return results
}
```

No variable hoisting. No indentation changes. The error handling is right next to the risky operation.

#### Declarative Code

But for declarative code, it **falls apart**:

```typescript
function ExtractResume(text: string) -> Resume | null {
  // ❌ Confusing: "Try to define a client?"
  let result = try {
    client "gpt-4o"
    prompt #"..."#
  } catch {
    _: TimeoutError => null
  }
  return result
}
```

This creates two problems:

1.  **Conceptual Mismatch**: We're not "trying" to *define* the client. We're defining a client that will be *tried* when called.
2.  **Indentation Tax**: Still forces indenting the function body and adding a return statement.

#### Why We Moved On

*   **Great for imperative, confusing for declarative**: The approach works beautifully for some code but feels wrong for others.
*   **Inconsistency**: Forces a "two mental models" approach—one for each style of code.

---

### Attempt 4: Function Modifiers (`function ... try`)

Add `try` as a modifier to function declarations.

```typescript
// Option A: After return type
function ExtractResume(text: string) -> Resume | null try {
  client "gpt-4o"
  prompt #"..."#
} catch {
  _: TimeoutError => null
}

// Option B: Before function keyword
try function ExtractResume(text: string) -> Resume | null {
  client "gpt-4o"
  prompt #"..."#
} catch {
  _: TimeoutError => null
}
```

#### Why We Moved On

*   **Syntactically awkward**: Both options feel "stranded" and unfamiliar.
*   **Grammar confusion**: `try function` reads like "attempt to define a function" rather than "define a function that attempts something."
*   **User feedback**: Multiple users said it "looks weird" and "feels odd."

---

### Attempt 5: Wrapper Functions

Forbid `catch` on declarative blocks. Instead, force users to create wrapper functions.

```typescript
// 1. Define the unsafe declarative function
function ExtractResumeUnsafe(text: string) -> Resume {
  client "gpt-4o"
  prompt #"..."#
}

// 2. Create a safe wrapper
function ExtractResume(text: string) -> Resume | null {
  try {
    return ExtractResumeUnsafe(text)
  } catch {
    _: TimeoutError => null
  }
}
```

#### Why We Moved On

*   **Viral Refactor**: To add error handling, you must rename the original function (breaking all callers, tests, and evals) or use different names.
*   **Boilerplate Explosion**: Every LLM call that needs error handling requires two functions.
*   **Tooling Loss**: IDE features like prompt previews might break when error handling is separated from the prompt definition.
*   **Irony**: Declarative blocks (LLM calls) are the **most likely to fail**, so forbidding direct error handling on them is counter-intuitive.

---

## 3. The Solution: Universal Catch

After exploring all these alternatives, we arrived at **Universal Catch**.

### The Core Concept

`catch` is an **operator that can attach to any block**:


*   **Function blocks**: `function F() { ... } catch { ... }`
*   **Block expressions**: `{ ... } catch { ... }`
*   **Control flow**: `for (...) { ... } catch { ... }` or `if (...) { ... } catch { ... }`
*   **Expressions**: `GetUser(id) catch { _ => null }`

The rule is simple and unified: **Attach `catch` to the thing that might fail.**

### The Unification

| Context | Syntax | Semantics |
|:--------|:-------|:----------|
| **Function Level** | `function F() { ... } catch { ... }` | Implicit Try (Scope = Function Body) |
| **Block Level** | `let x = { ... } catch { ... }` | Implicit Try (Scope = Block) |
| **Expression Level** | `let x = F() catch { ... }` | Inline Catch |
| **Explicit Try** | `let x = try { ... } catch { ... }` | Explicit Try (Signals Intent) |

**Key Insight**: `try` becomes **optional syntactic sugar**. It's semantically identical to `{ ... }`, but signals the reader: "This block expression exists specifically for error handling."

### Why This Works

#### For Declarative Code

No wrapping. No indentation. Just append the `catch`:

```typescript
function ExtractResume(text: string) -> Resume | null {
   client "gpt-4o"
   prompt #"Extract resume from {{ text }}"#
} catch {
   _: TimeoutError => null
}
```

The happy path remains **byte-for-byte identical**. You've added resilience without touching the original logic.

#### For Imperative Code

Use `try { ... }` when you want to signal intent, or just use `{ ... }` for brevity:

```typescript
function ProcessBatch(urls: string[]) -> Resume[] {
  // Optional 'try' signals: "This is specifically for error handling"
  let aggregator = try {
    MetricsAggregator.new()
  } catch {
    e => { log.warn(e); null }
  }
  
  let results = []
  
  for (url in urls) {
    let resume = ExtractResume(url)
    if (aggregator != null) {
      aggregator.record(resume)
    }
    results.append(resume)
  }
  return results
}
```

No hoisting. No viral refactors. Clean and explicit.

---

## 4. Learn by Example

Let's see Universal Catch in action across different scenarios.

### Scenario 1: The "Prototype to Production" Flow

You start with a clean LLM function:

```typescript
function ExtractResume(text: string) -> Resume {
   client "gpt-4o"
   prompt #"Extract resume from {{ text }}"#
}
```

In production, you realize timeouts happen. You want to return `null` instead of crashing.

**With Universal Catch**, you just append:

```typescript
function ExtractResume(text: string) -> Resume | null {
   client "gpt-4o"
   prompt #"Extract resume from {{ text }}"#
} catch {
   _: TimeoutError => null
}
```

**Zero lines changed.** No re-indentation. No hoisting. The git diff shows exactly what you added—just the error handling.

---

### Scenario 2: The "Resilient Loop"

You're processing a batch of URLs. One failure shouldn't crash the whole batch.

```typescript
function ExtractBatch(urls: string[]) -> Resume[] {
   let resumes = []

   for (url in urls) {
      let resume = ExtractResume(url)
      resumes.append(resume)
   } catch {
      // Per-iteration error handling
      e => {
         log.warn("Failed to extract resume", { url: url, error: e })
         // Loop continues to next item
      }
   }

   return resumes
}
```

The `catch` is attached to the **loop body**. If one iteration fails, you log it and keep going.

---

### Scenario 3: The "Pipeline" (Inline Catch)

You have a chain of operations where one step is optional:

```typescript
function ProcessUser(id: string) -> User {
   // If fetching preferences fails, just use defaults
   let prefs = GetUserPreferences(id) catch { _ => Preferences.default() }
   
   // If fetching profile fails, the whole function fails (no catch)
   let profile = GetUserProfile(id)
   
   return User { profile: profile, preferences: prefs }
}
```

Inline `catch` makes optional operations concise and clear.

---

### Scenario 4: Operator Precedence

The `catch` operator has the **lowest precedence** of all operators in BAML. This means it always applies to the entire preceding expression.

```typescript
function CalculateScore(a: int, b: int) -> int {
   // catch applies to (A() + B()), not just B()
   let sum = A() + B() catch { _ => 0 }
   
   // To catch only B(), use parentheses:
   let sum2 = A() + (B() catch { _ => 0 })
   
   // catch applies to the entire comparison
   let is_valid = CheckA() && CheckB() catch { _ => false }
   
   return sum
}
```

This design ensures that error handling scope is explicit and predictable. If you want to catch errors from a subexpression, use parentheses to make the scope clear.

---

### Scenario 5: The "Complex Logic" (Explicit Try Block)

You have a mix of safe and risky code in one function:

```typescript
function ComplexPipeline(data: string) -> Result {
   let safe_metadata = ExtractMetadata(data)
   
   // Signal intent: This specific section is risky
   let risky_analysis = try {
      let llm_result = AnalyzeWithLLM(data)
      let enhanced = EnhanceResult(llm_result)
      enhanced
   } catch {
      e => {
         log.error("LLM analysis failed", e)
         AnalysisResult.default()
      }
   }
   
   return Result {
      metadata: safe_metadata,
      analysis: risky_analysis
   }
}
```

Using an explicit `try { ... }` block expression signals to the reader: "This subsection is specifically being guarded."

---

## Conclusion

**Universal Catch** unifies error handling across declarative and imperative code with a simple rule: `catch` can attach to any block.


*   **Additive**: You never have to rewrite your happy path.
*   **Flexible**: Use function-level catch, block-level catch, or inline catch depending on your needs.
*   **Familiar**: `try` is optional but available when you want to signal intent.
*   **Designed for AI**: Probabilistic failures are first-class, not exceptional.

This is error handling designed for the "Prototype to Production" lifecycle—helping you move fast while exploring, and then snap on resilience when you're ready to deploy.
