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

Exception handling in BAML uses a **scoped catch** syntax that lets you handle errors declaratively without wrapping your code in try-blocks. This guide teaches you how to write resilient BAML functions.

## Quick Start

Add a `catch` block at the top of any function to handle errors:

```baml
function ExtractResume(text: string) -> Resume {
   catch {
      e: LlmError => { 
        return Resume { name: "Unknown", experience: [] } 
      }
   }

   client "openai/gpt-4o"
   prompt #"Extract resume from: {{ text }}"#
}
```

That's it! The `catch` block handles any LLM errors that occur below it, returning a fallback value instead of propagating the error.

## Core Concepts

### How Catch Blocks Work

A `catch` block must be the **first statement** in a scope (function, loop, or block). It handles all errors that occur in the code **below it** in that same scope.

**Think of it as an "open try"** — you write the catch at the top, and everything below is implicitly wrapped:

```baml
function Foo(arg: string) {
    catch {
        e: Err => { return fallback(arg) }
    }

    // Everything from here down is "protected" by the catch above
    let x = 1
    risky_operation()
    another_risky_operation()
} 
```

This is equivalent to (but more concise than):

```javascript
function Foo(arg) {
    try {
        let x = 1
        risky_operation()
        another_risky_operation()
    } catch (e) {
        if (e matches Err) return fallback(arg)
        throw e
    }
}
```

### Pattern Matching Errors

Use pattern matching (consistent with `match` expressions) to handle different error types:

```baml
catch {
   _: TimeoutError => { return retry_later() }
   e: ParseError => { 
      log("Failed to parse: " + e.message)
      return default_value() 
   }
   ApiError { code, message } => { 
      return Error { code: code, msg: message }
   }
}
```

**Pattern types:**

- `_: ErrorType` — Match the error type (ignore value)
- `e: ErrorType` — Match and bind the error instance to `e`
- `ErrorType { field1, field2 }` — Match and destructure error fields
- `name` — Named wildcard (matches any error)

### Named Wildcards

Use a named wildcard to catch all other errors:

```baml
catch {
   _: KnownError => { return fallback() }
   other => { 
      log.error("Unexpected error", other)
      throw other  // Re-throw to caller
   }
}
```

**Important:** If you don't provide a wildcard, BAML automatically adds one that propagates unhandled errors:

```baml
// What you write
catch {
   _: MyError => { return fallback() }
}

// What BAML compiles it to
catch {
   _: MyError => { return fallback() }
   __implicit__ => { throw __implicit__ }  // Auto-added
}
```

This means **all errors are always handled**—either explicitly by your handlers, or implicitly by propagation.

**Call stack preservation:** When errors are implicitly forwarded, their call stack information is preserved, making debugging easier even when errors propagate through multiple functions.

## Common Patterns

### Fallback Values

Return a safe default when an operation fails:

```baml
function GetUserProfile(userId: string) -> Profile {
   catch {
      _: NotFound => { 
         return Profile { 
            id: userId, 
            name: "Unknown User",
            active: false 
         }
      }
   }

   client "openai/gpt-4o"
   prompt #"Look up profile for user {{ userId }}"#
}
```

### Retry Logic

Handle transient errors by retrying:

```baml
function RobustExtract(text: string) -> Data {
   let attempts = 0
   
   while (true) {
      catch {
         _: TimeoutError => { 
            if (attempts < 3) {
               attempts += 1
               continue  // Retry
            }
            return Data.empty()  // Give up
         }
      }
      
      return ExtractData(text)  // Try the operation
   }
}
```

### Batch Processing

Handle errors for individual items without failing the entire batch:

```baml
class Success { value Resume }
class Failure { error string }
type Result = Success | Failure

function ProcessBatch(items: string[]) -> Result[] {
   let results = []

   for (item in items) {
     catch {
       other => { 
          results.append(Failure { error: other.message })
          continue  // Process next item
       }
     }
     
     let resume = ExtractResume(item)
     results.append(Success { value: resume })
  }

   return results
}
```

### Logging and Observability

Inspect errors before re-throwing:

```baml
function TrackedOperation() -> Data {
   catch {
      error => {
         metrics.increment("operation_errors")
         log.error("Operation failed", {
            error_type: error.type,
            message: error.message,
            context: current_context()
         })
         throw error  // Propagate to caller
      }
   }

   return expensive_operation()
}
```

### Graceful Degradation

Fall back to simpler approaches when the preferred method fails:

```baml
function SmartExtract(text: string) -> Data {
   catch {
      _: LlmError => {
         log("LLM failed, falling back to regex")
         return regex_extract(text)  // Simpler fallback
      }
   }

   client "openai/gpt-4o"
   prompt #"Extract structured data from: {{ text }}"#
}
```

## Advanced Features

### Nested Scopes

Inner catch blocks handle errors before outer ones:

```baml
function ProcessDocument(doc: string) -> Report {
   catch {
      _: CriticalError => { return Report.failed() }  // Outer catch
   }
   
   let sections = []
   
   for (section in doc.sections) {
      catch {
         _: ParseError => { 
            sections.append(Section.placeholder())
            continue  // Inner catch handles, continues loop
         }
      }
      
      sections.append(parse_section(section))
   }

   return Report { sections: sections }
}
```

**Control flow:** Inner catch handlers execute first. They can either handle the error (by returning a value) or re-throw it to outer catch blocks.

### Expression-Level Catch

Use catch blocks with expression blocks to handle errors at assignment:

```baml
function GetPrice(itemId: string) -> float {
   
   let price = {
      catch {
         _: ApiError => { 0.0 }  // Returns 0.0 to assign to 'price'
         _: AuthError => { return -1.0 }  // Returns -1.0 from FUNCTION
      }
      
      externalApi.getPrice(itemId)
   }

   return price * 1.2  // Tax applied (price is either 0.0 or actual)
}
```

**Key distinction:**

- `0.0` — Returns from the **block expression** (assigns to variable)
- `return -1.0` — Returns from the **function** (exits immediately)

### Variable Access

Catch blocks can access variables from outer scopes:

```baml
function ProcessWithContext(userId: string, data: Data) -> Result {
   let context = buildContext(userId)
   
   {
      catch {
         // ✅ Can access function parameters
         // ✅ Can access outer scope variables
         e: ProcessError => { 
            log.error("Failed for user " + userId, e)
            return Result.failure(context, e)
         }
      }
      
      return process(data, context)
   }
}
```

**Rule:** Catch blocks can only access variables declared in **outer scopes** (or function parameters), not variables declared below them.

### Strict Mode

Enable strict mode to require explicit handling of all known errors:

```baml
function SafeOperation() -> Data {
   catch(strict) {
      _: KnownError1 => { return fallback1() }
      _: KnownError2 => { return fallback2() }
      // Compiler error if any other known errors can occur!
   }

   return risky_operation()
}
```

**Default behavior** (without `strict`): Unknown errors are silently propagated via the implicit wildcard.

**With `strict`**: The compiler ensures you've explicitly handled all error types it can infer from the code below, preventing accidental omissions.

**Note:** Even in strict mode, the implicit wildcard is still added to handle dynamic errors that can't be known at compile time.

### Call Stacks and Error Context

Exceptions in BAML automatically capture call stack information, making debugging easier:

```baml
function A() -> Data {
   catch {
      e: MyError => {
         // Error 'e' contains the full call stack
         log.error("Error in A", {
            message: e.message,
            stack: e.stack,  // Full call stack from where error was thrown
            callSite: e.callSite  // Where this error was caught
         })
         throw e  // Stack is preserved when re-throwing
      }
   }
   B()  // If B throw, stack will include A -> B -> ...
}

function B() -> Data {
   C()  // If C throw, stack will include A -> B -> C -> ...
}

function C() -> Data {
   throw MyError("Something went wrong")  // Stack starts here
}
```

**Key points:**

- **Stack capture**: Every exception automatically captures where it was thrown
- **Preserved on propagation**: When errors are implicitly forwarded via the implicit wildcard, the call stack is preserved
- **Re-throw preserves stack**: Using `throw error` maintains the original stack trace
- **Call site tracking**: At minimum, the call site where an error is caught is always known

This makes debugging much easier, especially when errors propagate through multiple function calls:

```baml
function ProcessDocument(doc: string) -> Report {
   catch {
      e: ParseError => {
         // Even if this error came from deep in the call chain,
         // e.stack shows the full path: ProcessDocument -> ExtractSections -> ParseSection
         log.error("Failed to process document", {
            document: doc,
            error: e.message,
            stack: e.stack  // Complete trace
         })
         return Report.failed(e)
      }
   }
   
   ExtractSections(doc)  // Might call other functions that throw
}
```

## From Prototype to Production

One of BAML's design goals is making it easy to evolve code from quick prototypes to production-ready systems.

**Start with the happy path:**

```baml
function Extract() {
   client "openai/gpt-4o"
   prompt #"Extract data from document"#
}
```

**Add resilience without refactoring:**

```baml
function Extract() {
   catch { 
      _: TimeoutError => { return retry_with_timeout() }
      _: ParseError => { return fallback_parser() }
   }
   
   // Original code unchanged!
   client "openai/gpt-4o"
   prompt #"Extract data from document"#
}
```

Error handling is **additive**—you don't need to:

- Wrap existing code in try-blocks
- Change function signatures
- Update call sites
- Modify indentation

Just add the `catch` block at the top and you're done.

## Reference

### Syntax

**Scope-level catch:**

```baml
catch [( strict )] {
   Pattern => Handler
   Pattern => Handler
   ...
}
```

### Patterns

| Pattern | Description | Example |
|---------|-------------|---------|
| `_: ErrorType` | Match error type | `_: TimeoutError` |
| `e: ErrorType` | Bind error instance | `e: ParseError` |
| `ErrorType { fields }` | Destructure fields | `ApiError { code, msg }` |
| `name` | Named wildcard (matches any error) | `other` |

### Handlers

Handlers are blocks that execute when the pattern matches:

```baml
Pattern => { 
   // Can do multiple statements
   log(error)
   metrics.increment("errors")
   
   return fallback_value()  // Return from function
}

Pattern => { fallback_value() }  // Return from block expression

Pattern => { throw error }  // Re-throw to caller
```

### Placement Rules

**Scope-level catch:**

1. **Must be first** — Catch must be the first statement in its scope
2. **One per scope** — Only one catch block allowed per scope
3. **Any scope** — Can appear in functions, loops, if-blocks, or expression blocks

**Valid:**

```baml
function Foo() {
   catch { ... }  // ✅ Scope-level: First statement
   let x = 1
}
```

**Invalid:**

```baml
function Bar() {
   let x = 1
   catch { ... }  // ❌ Scope-level: Not first
}

function Baz() {
   catch { ... }  // ✅ First
   catch { ... }  // ❌ Duplicate scope-level catch
}
```

### Error Propagation

Errors propagate automatically via implicit wildcards:

```baml
function A() -> Data {
   catch {
      _: MyError => { return fallback() }
      // Other errors propagate automatically
   }
   B()  // Might throw OtherError
}

function B() -> Data {
   throw OtherError()  // Propagates through A's implicit wildcard
}
```

To explicitly handle all errors without propagating:

```baml
catch {
   _: MyError => { return fallback1() }
   other => { return fallback2() }  // Catches everything else
}
```

## Best Practices

### ✅ Do

- **Place catch blocks at the top** of scopes for clarity
- **Use pattern matching** to handle different error types differently
- **Return fallback values** for recoverable errors
- **Log before re-throwing** for observability
- **Use strict mode** in production code to prevent accidental omissions

### ❌ Don't

- **Don't catch and ignore** without logging

  ```baml
  catch { error => { } }  // Silent failure - bad!
  ```

- **Don't catch everything** unless you have a good reason

  ```baml
  catch { other => { return null } }  // Hides all errors
  ```

- **Don't place catch blocks in the middle** of scopes

  ```baml
  let x = 1
  catch { ... }  // Compile error
  ```

## Examples

### Complete Example: Resume Parser with Error Handling

```baml
class Resume {
  name string
  email string
  experience string[]
  skills string[]
}

class ParsingResult {
  success bool
  data Resume?
  error string?
}

function ParseResume(text: string) -> Resume {
   client "openai/gpt-4o"
   prompt #"
      Extract resume information from the following text.
      Return a structured format with name, email, experience, and skills.
      
      Text:
      {{ text }}
   "#
}

function SafeParseResume(text: string) -> ParsingResult {
   catch {
      _: TimeoutError => {
         return ParsingResult {
            success: false,
            data: null,
            error: "LLM timeout - please retry"
         }
      }
      e: ParseError => {
         return ParsingResult {
            success: false,
            data: null,
            error: "Could not parse: " + e.message
         }
      }
      other => {
         log.error("Unexpected error in ParseResume", other)
         metrics.increment("parse_resume_unknown_errors")
         return ParsingResult {
            success: false,
            data: null,
            error: "Internal error - please contact support"
         }
      }
   }

   let resume = ParseResume(text)
   return ParsingResult { success: true, data: resume }
}

function ParseBatchResumes(texts: string[]) -> ParsingResult[] {
   let results = []
   
   for (text in texts) {
      // Each resume gets independent error handling
      results.append(SafeParseResume(text))
   }
   
   return results
}
```

This example demonstrates:

- Specific error handling for known error types
- User-friendly error messages
- Logging and metrics for unknown errors
- Batch processing where individual failures don't stop the batch

## Why not try/catch?

You might wonder why BAML uses this scoped catch syntax instead of the familiar `try/catch` blocks found in other languages.

The decision comes down to **ergonomics for AI engineering**.

### 1. Zero-Refactor Resilience

In traditional languages, moving from a prototype ("happy path" code) to a production-ready system (resilient code) requires structural refactoring:
- You must wrap risky code in `try` blocks
- This forces you to indent all the code inside
- You often need to move variable declarations outside the block to keep them in scope

With scoped catch, error handling is strictly **additive**. You simply paste a `catch` block at the top of the function. Your existing logic stays exactly where it is—no indentation changes, no variable hoisting, and no "diff noise."

This is particularly important for AI-generated code, where minimizing the size of diffs helps LLMs maintain context and reduce errors.

### 2. Declarative "Open Try"

BAML's syntax acts like an "open try"—it declares "here is how we handle failures in this scope" right at the start.

- **Traditional**: "Try to do X, Y, Z... and if something fails, jump down here."
- **BAML**: "If anything fails in this scope, handle it like this. Now, do X, Y, Z."

This puts the failure recovery logic front-and-center rather than burying it at the bottom of the function.

### 3. Clean Variable Access

Because the catch block sits at the top of the scope, it naturally has access to all function parameters and variables declared in outer scopes. This makes it easy to construct rich error context or fallback values without fighting scoping rules.

---


## Updates

### Throwing Arbitrary Types
We are currently evaluating two approaches for handling `throw` with arbitrary values (primitives, structs):

1.  **Universal Wrapper**: All thrown values are wrapped in an `Exception<T>` envelope.
2.  **Interface Restriction**: Only types implementing an `Error` interface can be thrown.

See [updates/primitive-and-arbitrary-types](updates/primitive-and-arbitrary-types) for the detailed trade-off analysis.

### Inline `.catch()`

Was removed. See [updates/inline-catch](updates/inline-catch) for the detailed trade-off analysis.
