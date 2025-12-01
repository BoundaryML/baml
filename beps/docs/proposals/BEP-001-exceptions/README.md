---
id: BEP-001
title: "Exception Handling"
shepherds: Vaibhav Gupta <vbv@boundaryml.com>
status: Draft
created: 2025-11-20
---

Exception handling in BAML uses a **scoped catch** syntax that lets you handle errors declaratively without wrapping your code in try-blocks. This guide teaches you how to write resilient BAML functions.

## Quick Start

Add a `catch` block at the top of any function to handle errors:

```baml
function ExtractResume(text: string) -> Resume {
   catch {
      LlmError(e) => { 
        return Resume { name: "Unknown", experience: [] } 
      }
   }

   client "openai/gpt-4o"
   prompt #"Extract resume from: {{ text }}"#
}
```

That's it! The `catch` block handles any LLM errors that occur below it, returning a fallback value instead of propagating the error.

**Alternative: Inline catch** — You can also handle errors at specific call sites:

```baml
function GetProfile(userId: string) -> Profile {
   let profile = FetchProfile(userId).catch({
      NotFound() => { Profile.default(userId) }
   })
   
   return profile
}
```

Use scope-level catch (at the top) for multiple operations, or inline `.catch()` for specific calls.

## Core Concepts

### How Catch Blocks Work

A `catch` block must be the **first statement** in a scope (function, loop, or block). It handles all errors that occur in the code **below it** in that same scope.

**Think of it as an "open try"** — you write the catch at the top, and everything below is implicitly wrapped:

```baml
function Foo(arg: string) {
    catch {
        Err(e) => { return fallback(arg) }
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

Use pattern matching to handle different error types:

```baml
catch {
   TimeoutError() => { return retry_later() }
   ParseError(e) => { 
      log("Failed to parse: " + e.message)
      return default_value() 
   }
   ApiError { code, message } => { 
      return Error { code: code, msg: message }
   }
}
```

**Pattern types:**

- `ErrorType()` — Match the error type
- `ErrorType(e)` — Match and bind the error instance to `e`
- `ErrorType { field1, field2 }` — Match and destructure error fields

### Named Wildcards

Use a named wildcard to catch all other errors:

```baml
catch {
   KnownError() => { return fallback() }
   other => { 
      log.error("Unexpected error", other)
      throws other  // Re-throw to caller
   }
}
```

**Important:** If you don't provide a wildcard, BAML automatically adds one that propagates unhandled errors:

```baml
// What you write
catch {
   MyError() => { return fallback() }
}

// What BAML compiles it to
catch {
   MyError() => { return fallback() }
   __implicit__ => { throws __implicit__ }  // Auto-added
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
      NotFound() => { 
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
         TimeoutError() => { 
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
         throws error  // Propagate to caller
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
      LlmError() => {
         log("LLM failed, falling back to regex")
         return regex_extract(text)  // Simpler fallback
      }
   }

   client "openai/gpt-4o"
   prompt #"Extract structured data from: {{ text }}"#
}
```

### Call-Site Error Handling

Use inline `.catch()` for handling errors on specific function calls:

```baml
function GetUserProfile(userId: string) -> Profile {
   // Primary data source with fallback
   let userData = FetchFromDatabase(userId).catch({
      DatabaseError() => { FetchFromCache(userId) }
   })
   
   // Optional enrichment - don't fail if it doesn't work
   let enriched = EnrichProfile(userData).catch({
      other => { userData }  // Return un-enriched on any error
   })
   
   return enriched
}
```

This keeps error handling close to the operation, making the code more readable when different calls need different error handling strategies.

## Advanced Features

### Inline Catch at Call Sites

You can append `.catch()` to any function call or expression block to handle errors at the call site:

```baml
function ProcessData(input: string) -> Result {
   // Handle errors for this specific call
   let data = FetchData(input).catch({
      NetworkError() => { Data.fromCache(input) }
      TimeoutError() => { Data.empty() }
   })
   
   return process(data)
}
```

This is equivalent to:

```baml
function ProcessData(input: string) -> Result {
   let data = {
      catch {
         NetworkError() => { Data.fromCache(input) }
         TimeoutError() => { Data.empty() }
      }
      FetchData(input)
   }
   
   return process(data)
}
```

**When to use inline catch:**

- Handling errors for a **specific call** without affecting other code
- Quick fallbacks for individual operations
- Keeping error handling close to the operation

**When to use scope-level catch:**

- Handling errors for **multiple operations** in a scope
- Complex error handling logic
- Shared error handling across a function

#### Chaining Multiple Operations

Inline catch works well when building pipelines of operations:

```baml
function GetUserData(userId: string) -> UserData {
   // Fetch user with fallback
   let user = fetchUser(userId).catch({
      NotFound() => { User.guest(userId) }
   })
   
   // Enrich with error handling
   let enriched = enrichUser(user).catch({
      EnrichmentError() => { user }  // Return un-enriched user
   })
   
   return enriched
}
```

#### Mixing Scope and Inline Catches

You can combine scope-level and inline catches for fine-grained control:

```baml
function ComplexOperation(id: string) -> Result {
   catch {
      // Scope-level catch for unexpected errors
      other => {
         log.error("Unexpected error in ComplexOperation", other)
         throws other
      }
   }
   
   // Inline catch for specific operation
   let primary = FetchPrimary(id).catch({
      NotFound() => { null }
   })
   
   // Different handling for different calls
   let secondary = FetchSecondary(id).catch({
      NotFound() => { Secondary.default() }
      RateLimited() => { Secondary.fromCache(id) }
   })
   
   return combine(primary, secondary)
}
```

**Order of execution:** Inline catches are evaluated first, then scope-level catches. If an inline catch re-throws (via `throws`), the error propagates to the scope-level catch.

#### Expression Blocks with Inline Catch

You can also use `.catch()` on expression blocks:

```baml
function Calculate() -> float {
   let result = {
      let x = compute_x()
      let y = compute_y()
      x / y
   }.catch({
      DivisionByZero() => { 0.0 }
   })
   
   return result
}
```

### Nested Scopes

Inner catch blocks handle errors before outer ones:

```baml
function ProcessDocument(doc: string) -> Report {
   catch {
      CriticalError() => { return Report.failed() }  // Outer catch
   }
   
   let sections = []
   
   for (section in doc.sections) {
      catch {
         ParseError() => { 
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
         ApiError() => { 0.0 }  // Returns 0.0 to assign to 'price'
         AuthError() => { return -1.0 }  // Returns -1.0 from FUNCTION
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
         ProcessError(e) => { 
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
      KnownError1() => { return fallback1() }
      KnownError2() => { return fallback2() }
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
      MyError(e) => {
         // Error 'e' contains the full call stack
         log.error("Error in A", {
            message: e.message,
            stack: e.stack,  // Full call stack from where error was thrown
            callSite: e.callSite  // Where this error was caught
         })
         throws e  // Stack is preserved when re-throwing
      }
   }
   B()  // If B throws, stack will include A -> B -> ...
}

function B() -> Data {
   C()  // If C throws, stack will include A -> B -> C -> ...
}

function C() -> Data {
   throw MyError("Something went wrong")  // Stack starts here
}
```

**Key points:**

- **Stack capture**: Every exception automatically captures where it was thrown
- **Preserved on propagation**: When errors are implicitly forwarded via the implicit wildcard, the call stack is preserved
- **Re-throw preserves stack**: Using `throws error` maintains the original stack trace
- **Call site tracking**: At minimum, the call site where an error is caught is always known

This makes debugging much easier, especially when errors propagate through multiple function calls:

```baml
function ProcessDocument(doc: string) -> Report {
   catch {
      ParseError(e) => {
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
      TimeoutError() => { return retry_with_timeout() }
      ParseError() => { return fallback_parser() }
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

**Inline catch at call site:**

```baml
expression.catch({
   Pattern => Handler
   Pattern => Handler
   ...
})
```

Where `expression` can be:

- A function call: `FetchData(id).catch({ ... })`
- An expression block: `{ /* code */ }.catch({ ... })`

### Patterns

| Pattern | Description | Example |
|---------|-------------|---------|
| `ErrorType()` | Match error type | `TimeoutError()` |
| `ErrorType(e)` | Bind error instance | `ParseError(e)` |
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

Pattern => { throws error }  // Re-throw to caller
```

### Placement Rules

**Scope-level catch:**

1. **Must be first** — Catch must be the first statement in its scope
2. **One per scope** — Only one catch block allowed per scope
3. **Any scope** — Can appear in functions, loops, if-blocks, or expression blocks

**Inline catch:**

- Can appear **anywhere** an expression is valid
- Can be used **multiple times** on different expressions
- Attached with `.catch({ ... })` directly to the expression

**Valid:**

```baml
function Foo() {
   catch { ... }  // ✅ Scope-level: First statement
   let x = 1
   
   let y = Bar().catch({ ... })  // ✅ Inline: Anywhere
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
      MyError() => { return fallback() }
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
   MyError() => { return fallback1() }
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

function ParseResume(text: string) -> ParsingResult {
   catch {
      TimeoutError() => {
         return ParsingResult {
            success: false,
            data: null,
            error: "LLM timeout - please retry"
         }
      }
      ParseError(e) => {
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

   client "openai/gpt-4o"
   prompt #"
      Extract resume information from the following text.
      Return a structured format with name, email, experience, and skills.
      
      Text:
      {{ text }}
   "#
}

function ParseBatchResumes(texts: string[]) -> ParsingResult[] {
   let results = []
   
   for (text in texts) {
      // Each resume gets independent error handling
      results.append(ParseResume(text))
   }
   
   return results
}
```

This example demonstrates:

- Specific error handling for known error types
- User-friendly error messages
- Logging and metrics for unknown errors
- Batch processing where individual failures don't stop the batch

---

## Learn More

- **Language Reference**: Full BAML syntax documentation
- **Error Types**: Built-in error types in BAML
- **Testing**: How to test error handling in BAML functions
- **Observability**: Integrating error tracking and monitoring
