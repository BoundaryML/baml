---
id: BEP-001
title: "Exception Handling"
shepherds: Vaibhav Gupta <vbv@boundaryml.com>
status: Draft
created: 2025-11-20
---

!!! note ""
    Leave comments on either

      - [internal boundary slack thread](https://gloo-global.slack.com/archives/C0958DV7YPL/p1764615609844069)
      - [public github discussion](https://github.com/orgs/BoundaryML/discussions/2761)

This guide teaches you how to write resilient BAML code using **Trailing Catch**.

BAML's error handling is designed for the "Prototype to Production" lifecycle. It allows you to write clean, happy-path code first, and then "attach" error handling logic later without rewriting or re-indenting your functions.

## 1. The Basics: Function-Level Catch

The simplest way to handle errors is to attach a `catch` block to the end of a function.

Imagine a simple function that calls an LLM:

```baml
function ExtractResume(text: string) -> Resume {
   client "openai/gpt-4o"
   prompt #"Extract resume from: {{ text }}"#
}
```

To make this resilient, you don't wrap the code. You just append a `catch` block:

```baml
function ExtractResume(text: string) -> Resume {
   client "openai/gpt-4o"
   prompt #"Extract resume from: {{ text }}"#
} catch {
   // This block handles ANY error from the function body above
   e: LlmError => { 
     return Resume { name: "Unknown", experience: [] } 
   }
}
```

**Key Concept**: The `catch` block acts as an "Implicit Try". It treats the entire function body as if it were inside a `try` block.

## 2. Pattern Matching Errors

You can handle different types of errors differently using pattern matching. This works just like the `match` expression.

```baml
} catch {
   // 1. Match by Type (ignore details)
   _: TimeoutError => { return default_value() }

   // 2. Match and Bind (use the error object)
   e: ParseError => { 
      log("Failed to parse: " + e.message)
      return default_value() 
   }

   // 3. Destructure (extract specific fields)
   ApiError { code, message } => { 
      return Error { code: code, msg: message }
   }

   // 4. Wildcard (catch everything else)
   other => {
      log.error("Unexpected error", other)
      throw other // Re-throw to caller
   }
}
```

## 3. Expressions & Type Safety

In BAML, `if` blocks and `{ ... }` blocks can be used as **expressions** that return values.

When you attach a `catch` block to an expression, the catch block **must return a value** compatible with the expression's type.

### Type Inference

The type of the entire expression is the union of:
1.  The type returned by the happy path.
2.  The type returned by the catch block.

```baml
// 'result' will be inferred as: string | null
let result = {
   Extract(text) // returns string
} catch {
   _: Error => null // returns null
}
```

### Inline Catch

Since `catch` can be attached to **any expression**, you can use it for concise, one-line error handling without creating a block.

```baml
// Handle errors for a single function call
let user = GetUser(id) catch { _ => null }

// Handle errors for a pipeline
let data = Extract(text) catch { 
   _: Timeout => null 
}
```

This is extremely useful for "optional" operations where a failure should just result in a null or default value.

### Type Safety Rules

When you attach a `catch` block to an expression, BAML enforces strict type safety rules to prevent runtime errors.

#### Rule 1: Catch Blocks Must Return Values for Expressions

If you use a block as an expression (e.g., assigning it to a variable), it is a **compiler error** if the catch block does not return a value.

```baml
// ❌ Compile Error: Expression must return a value
let result = {
   Extract(text)
} catch {
   e => {
      log(e)
      // Error: Missing return value!
      // We cannot assign 'void' to 'result'
   }
}
```

This ensures type safety. You cannot accidentally leave a variable uninitialized or undefined by forgetting to return a fallback value in your error handler.

#### Rule 2: Type Inference for Expressions with Catch

For **inferred types** (when you don't explicitly annotate the variable), BAML infers the type as the **union** of all possible return paths.

Looking back at our earlier examples:

```baml
// Inferred type: string | null
let user = GetUser(id) catch { _ => null }

// Inferred type: Data | null
let data = Extract(text) catch { 
   _: Timeout => null 
}

// Inferred type: string | null
let result = {
   Extract(text) // returns string
} catch {
   _: Error => null // returns null
}
```

BAML looks at what each path returns and creates a union type automatically.

#### Rule 3: Explicit Type Annotations Require Compatibility

If you **explicitly annotate** a variable's type, the catch block **must** return a value compatible with that type.

```baml
// ✅ Valid: catch block returns string (compatible with string)
let result: string = Extract(text) catch { 
   _ => "default" 
}

// ❌ Compile Error: Type mismatch
let result: string = Extract(text) catch { 
   _ => null  // Error: Cannot assign 'null' to type 'string'
}

// ✅ Valid: Explicit union type allows null
let result: string | null = Extract(text) catch { 
   _ => null 
}
```

This same rule applies to function return types:

```baml
// ❌ Compile Error: Function returns Data, but catch returns null
function Extract(text: string) -> Data {
    client "openai/gpt-4o"
    prompt #"Extract: {{ text }}"#
} catch {
    _: Timeout => null  // Error: null is not assignable to Data
}

// ✅ Valid: Function signature allows null
function Extract(text: string) -> Data | null {
    client "openai/gpt-4o"
    prompt #"Extract: {{ text }}"#
} catch {
    _: Timeout => null  // OK: null is in the union type
}
```

**Key Insight**: When you add a catch block that returns a different type, you must update the function signature (or variable annotation) to reflect that possibility. This forces you to be explicit about the fact that your function might return an error value.

## 4. Control Flow Integration

Trailing catch isn't just for functions. You can attach it to **control flow statements** like `for` loops and `if` statements.

### Resilient Loops (Batch Processing)

A common pattern in AI engineering is processing a batch of items where some might fail. You don't want one failure to crash the whole batch.

By attaching `catch` to a `for` loop, you create a **Resilient Loop**. The catch block runs *per iteration*.

```baml
function ExtractBatch(urls: string[]) -> Resume[] {
   let resumes = []

   for (url in urls) {
      // If this throws...
      let resume = ExtractResume(url)
      resumes.append(resume)

   } catch {
      // ...we catch it here, log it, and the loop CONTINUES!
      e => {
         log.warn("Failed to extract resume", { url: url, error: e.message })
      }
   }

   return resumes
}
```

### Conditionals

You can attach catch blocks to any `if` or `else` block.

**Simple If:**

```baml
if (use_fast_model) {
   ExtractFast(text)
} catch {
   // If fast model fails, try slow model
   _: ModelError => ExtractSlow(text)
}
```

**If / Else:**

You can also handle errors independently for each branch:

```baml
if (use_fast_model) {
   ExtractFast(text)
} catch {
   _: ModelError => ExtractSlow(text)
} else {
   ExtractReasoning(text)
} catch {
   e => PartialResult(text)
}
```

**Handling Errors in Conditions**:

The `catch` block is attached to the *body* of the `if`, not the condition. If you need to handle errors in the condition itself, use **inline catch** (see Section 3):

```baml
// ✅ Catch errors from the condition
if (RiskyCondition() catch { _ => false }) {
   DoSomething()
}
```

If you want to catch errors from **both** the condition *and* the body, wrap the entire statement in a block:

```baml
{
   if (RiskyCondition()) { 
      DoSomething()
   }
} catch {
   e => log("Caught error from condition or body")
}
```

## 5. Scoping and Data Access

One of the biggest challenges in error handling is accessing the data you need to log or recover.

**Rule**: A `catch` block can access any variable defined **before** the scope it is attached to. It cannot access variables defined **inside** the scope (because the scope was interrupted).

```baml
function ProcessUser(userId: string) {
   // ✅ Defined BEFORE the block
   let context = GetContext(userId)

   {
      // ❌ Defined INSIDE the block
      let result = RiskyOp(context)
      return result
   } catch {
      e => {
         // We can access 'userId' and 'context' here!
         log.error("Failed to process", { 
            user: userId, 
            ctx: context, 
            error: e 
         })
      }
   }
}
```

This pattern allows you to define variables, start a block, and then handle errors using those variables naturally, without needing to hoist definitions outside the scope.

## 6. Design Rationale

Why did we invent a new syntax instead of using `try/catch` or `Result` types?

The answer lies in the **nature of AI code**.

### 6.1 The Probabilistic Reality

In traditional software, exceptions are *exceptional*. `JSON.parse` failing is an anomaly. The disk being full is a crisis.

**In AI Engineering, failure is just Tuesday.**

When you introduce an LLM, you introduce **probabilistic failure** at every step.
*   The model might refuse the request.
*   It might hallucinate a field.
*   It might timeout.
*   It might return valid JSON that misses the point entirely.

Error handling isn't just for "crashing gracefully"—it is **core control flow**. You need to retry, re-prompt, switch models, or fallback to heuristics constantly.

Because failure is the default state, the syntax for handling it must be as low-friction as an `if` statement. If error handling is painful (like nesting 3 layers deep), developers won't do it enough.

### 6.2 The "Refactoring Tax"

When you move from a "Vibe Coding" prototype to a production system, traditional languages punish you.

**The Scenario**: You have a clean, working function.
```typescript
function Extract(text) {
  const client = new Client();
  return client.run(text);
}
```

Now you want to handle a timeout.

**The `try/catch` Tax**:
You must perform a **Structural Refactor**.
1.  Wrap everything in `try { ... }`.
2.  **Indent** every single line of your happy path.
3.  **Hoist** variables outside the block if you need them later.

```diff
function Extract(text) {
+ try {
    const client = new Client();
    return client.run(text);
+ } catch (e) {
+   return null;
+ }
}
```
In git, this looks like you rewrote the whole function. The "Happy Path" is now visually subservient to the error handling.

**The "Return Type" Tax (The Viral Refactor)**:

In languages like Go, Rust, or even TypeScript (if you return `Data | Error`), adding error handling changes the **signature** of your function.

1.  You change `Extract(text) -> Data` to `Extract(text) -> Result<Data, Error>`.
2.  Now the caller's code `let data = Extract(text)` is broken. It now holds a `Result` (or a union).
3.  You must update the caller to unwrap the result or check the type.
4.  If the caller can't handle the error, *it* must also change its return type to pass the error up.

This ripples up the entire call stack. Suddenly, adding a simple retry policy to one function requires touching 10 files and updating every test.

### 6.3 The BAML Solution: Additive Resilience

BAML is designed to let you evolve code from **Prototype** to **Production** without paying these taxes.

We believe that **Error Handling should be Additive**.

When you want to harden your function, you shouldn't have to touch the happy path at all. You just **append** the resilience logic:

```baml
function Extract(text: string) -> Data | null {
    // 👇 This code is UNTOUCHED. No indentation changes. No hoisting.
    client "openai/gpt-4o"
    prompt #"Extract from: {{ text }}"#
} catch {
    // 👇 You just added this.
    _: Timeout => null
}
```

**Note**: When you add a catch block that returns a different type (like `null`), the function's return type becomes the union of both types (`Data | null`). For inferred types or expressions, BAML handles this automatically.

This respects your workflow. It lets you move fast when exploring, and then "snap on" safety features when stabilizing.

### 6.4 Built for Agents

BAML is designed not just for human engineers, but for **AI Agents** writing code.

**Additive syntax is safer for LLMs.**

*   **Structural Edits are Risky**: Asking an LLM to "wrap this code in a try/catch block" requires it to rewrite the entire function body. It might accidentally drop a line, hallucinate a change, or mess up indentation.
*   **Additive Edits are Safe**: Asking an LLM to "handle errors for this function" in BAML means it just generates a few lines to **append** at the end. The original logic remains byte-for-byte identical.

This makes BAML the ideal language for the next generation of Agentic IDEs.

### 6.5 Trade-offs

We recognize that this syntax can feel awkward in specific edge cases, particularly when you want to catch errors from an **entire** `if` statement (condition + body).

To do this, you must wrap the `if` in a block:

```baml
{
  if (RiskyCondition()) { ... }
} catch { ... }
```

We decided against introducing a `try` keyword just for this edge case. The benefits of **Additive Resilience** (never having to re-indent your happy path) outweigh the occasional awkwardness of wrapping a complex control flow statement.
