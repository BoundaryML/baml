# Scoped Catch Syntax Proposal

## Overview

This proposal introduces a **scoped catch** syntax that acts as a declarative error handler for the current scope.

The core idea is simple: **A catch block implicitly wraps the remainder of its scope.**

### Constraint: First Statement Only

The `catch` block **must** be the first statement in the scope. It cannot be preceded by other statements (like variable declarations) in the same block.

### Mental Model: "Open Try"

You can view this syntax as syntactic sugar for a traditional `try-catch` block where the `try` automatically extends from the catch block to the end of the scope.

**What you write (BAML):**
```baml
function Foo(arg: string) {
    // Catch must be the first statement
    catch {
        e: Err => { return arg } // Can access parameters or outer variables
    }

    // Everything below is effectively inside a 'try'
    let x = 1;
    risky_operation_1()
} 
```

**How to think about it (Desugaring):**
```javascript
function Foo(arg) {
    try {
        let x = 1;
        risky_operation_1()
    } catch (e) {
        if (e matches Err) return arg;
        throw e;
    }
}
```

### From Prototype to Production

This syntax is designed to support the lifecycle of AI engineering: moving from fragile prototypes to resilient production systems without rewriting code.

1.  **Prototype (Happy Path)**: You write linear code to test your prompts.
    ```baml
    function Extract() {
       client "openai"
       prompt #"..."#
    }
    ```

2.  **Production (Resilient)**: To handle timeouts or refusals, you don't need to wrap/indent your logic or change call sites. You simply **add** a `catch` block at the top.
    ```baml
    function Extract() {
       catch { ... } // <--- Just added this
       
       // Original code remains untouched
       client "openai"
       prompt #"..."#
    }
    ```

This makes error handling an **additive** layer rather than a structural refactor.

## Syntax Examples

### Function-level Catch

```baml
// Data definitions
class Resume {
  name string
  experience string[]
}

// 1. Function-level Catch (Declarative LLM Function)
function ExtractResume(text: string) -> Resume {
   // Catch block handles LLM failures, Parsing errors, etc.
   // Must be the first statement in the scope
   catch {
      // Return a default/fallback value on failure
      e: LlmError => { 
        return Resume { name: "Unknown", experience: [] } 
      }
   }

   // Specifies the LLM client
   client "openai/gpt-4o"

   // The actual prompt (BAML handles the execution and parsing)
   prompt #"
     Extract the resume details from the text below:
     {{ text }}
   "#
}
```

### Scope-level Catch

```baml
// BAML doesn't have a built-in Result type, so we define one
class Success {
  value Resume
}
class Failure {
  error string
}
type Result = Success | Failure

// 2. Scope-level Catch (Imperative Logic)
function ProcessBatch(items: string[]) -> Result[] {
   let results = []

   for (item in items) {
     catch {
       // Capture item-specific errors without failing the batch
       other => { 
          results.append(Failure { error: other.message }) 
          continue 
       }
     }
     
     // Call the declarative function
     let processed = ExtractResume(item)
     results.append(Success { value: processed })
  }

   return results
}
```

### Expression-level Catch (Block Returns)

A key feature is the ability to distinguish between returning a value for the block (assignment) and returning from the function.

```baml
function GetPrice(itemId: string) -> float {
   
   let price = {
      catch {
         // ✅ Expression return: Returns 0.0 to 'price' variable (fallback)
         _: ApiError => { 0.0 }
         
         // ✅ Function return: Exits the entire function immediately
         _: AuthError => { return -1.0 } 
      }
      
      externalApi.getPrice(itemId)
   }

   return price * 1.2 // tax applied to price (0.0 or actual)
}
```

## Design Rationale

### Primary Benefits

1. **Minimal Diff Overhead**: When making code error-prone, no need for:
    - Adding a `try` keyword at the beginning or at every call site (unlike Swift/Rust)
    - Changing the function signature to declare throws (unlike Java/Swift)
    - Indenting the entire scope
    - Adding a `catch` block at the end
    - This is crucial for AI agents editing code. Transitioning from non-error-handled to error-handled code is a local change (adding the catch block) rather than a global one (updating all call sites and signatures), preventing massive cascading diffs.

2. **Scoped Variable Access**: The catch block has access to all parameters and variables declared above it in the scope, making context more explicit and accessible

3. **Declarative Error Handling**: By placing error handling at the top of a scope, it acts as a declaration of "what can go wrong" rather than wrapping code

### Prototyping to Production for Agents

Building AI agents (code that orchestrates LLMs) typically follows a distinct lifecycle:

1. **Prototyping**: Rapidly iterating on prompts and logic to get the "happy path" working. At this stage, error handling is noise; you want to see if the idea works.
2. **Productionizing**: The transition to production is almost entirely about adding reliability—handling timeouts, refusals, parsing errors, and edge cases.

In traditional languages, this transition is painful. Adding error handling often requires:

- Wrapping large blocks of code in `try/catch` (indentation changes).
- Changing function signatures to propagate errors (breaking callers).
- Refactoring linear logic into complex control flow.

With **Scoped Catch**, "productionizing" an agent function is strictly **additive**: you simply paste a `catch` block at the top of the scope. The original prototyping logic remains untouched, unindented, and linear. This lowers the activation energy for adding reliability, ensuring that "quick prototypes" can actually evolve into robust production systems without a rewrite.

## Comparison to Other Languages

### Similar To

#### Swift's `defer`
- **Similarity**: Like Swift's `defer`, this appears at scope entry but executes under special conditions
- **Difference**: `defer` always executes on scope exit; this only executes on errors
- **Swift Example**:
  ```swift
  func processFile() {
      let file = openFile()
      defer { file.close() }  // Appears early, executes on exit
      // ... code
  }
  ```

#### Go's Error Handling Philosophy
- **Similarity**: Explicit error handling without exceptions
- **Difference**: Go returns errors as values; this catches errors declaratively
- **Go Example**:
  ```go
  result, err := doSomething()
  if err != nil {
      // handle immediately
  }
  ```

#### Rust's `?` Operator with Match
- **Similarity**: Pattern matching on error types
- **Difference**: Rust requires explicit `?` at call sites; this auto-infers
- **Rust Example**:
  ```rust
  match operation() {
      Ok(value) => { /* success */ },
      Err(MyError) => { /* handle */ },
      Err(e) => { /* other errors */ }
  }
  ```

### Different From

#### Traditional Try-Catch (Java, Python, TypeScript, Swift)
- **Traditional**: Wraps code with `try`, catch appears at scope end
- **This Proposal**: No wrapping, catch appears at scope beginning
- **Traditional Example** (Java):
  ```java
  try {
      // indented code
  } catch (MyError e) {
      // handler at end
  }
  ```

#### Go's Inline Error Checking
- **Go**: Error checking happens inline at each call site
- **This Proposal**: Centralized error handling at scope beginning
- **Go requires**:
  ```go
  if err != nil { return err }
  ```
  after each fallible operation

#### Python's Context Managers (`with`)
- **Python**: `with` handles setup/teardown, not error handling
- **This Proposal**: Specifically for error handling
- **Python Example**:
  ```python
  with open('file') as f:
      # code
  # cleanup happens automatically
  ```

## Key Features

### Named Wildcard Pattern

A distinguishing feature of this syntax is the **named wildcard pattern** for error propagation:

```baml
function Foo(param: T) -> Bar {
   catch {
      _: MyError => { return Bar.default() }
      _: DatabaseError => { return Bar.fromCache() }
      // Named wildcard captures all other errors
      other => { 
         log.error("Unexpected error in Foo", other)
         throws other 
      }
   }
   // ... code
}
```

#### Implicit Wildcard Desugaring

**Critical Implementation Detail**: The wildcard pattern is **implicitly added** to every catch block via compiler desugaring.

**What you write**:
```baml
function Foo() -> Bar {
   catch {
      _: MyError => { return Bar.default() }
   }
   // code
}
```

**What the compiler generates**:
```baml
function Foo() -> Bar {
   catch {
      _: MyError => { return Bar.default() }
      __implicit_other__ => { throws __implicit_other__ }  // Implicitly added
   }
   // code
}
```

**Benefits of Implicit Desugaring**:
- **Cleaner Syntax**: Don't need to write `other => { throws other }` everywhere
- **Safety by Default**: All errors are always handled or propagated
- **Future-Proof**: Handles dynamic exceptions (e.g., from future `eval` feature) gracefully
- **Explicit When Needed**: Developers can still write explicit wildcards for logging/inspection

**When to Write Explicit Wildcards**:
```baml
// Explicit wildcard for logging before propagation
catch {
   _: KnownError => { return fallback() }
   other => { 
      log.error("Unexpected error", other)
      metrics.increment("unknown_errors")
      throws other 
   }
}
```

**Benefits**:
- **Explicit Propagation**: Makes it clear that unhandled errors will propagate
- **Error Access**: The name (e.g., `other`) provides access to the error instance
- **Logging/Debugging**: Can inspect or log errors before re-throwing
- **Type Safety**: Compiler knows that all possible errors are handled
- **Dynamic Code Support**: Handles errors from dynamic code execution (e.g., `eval`)

**Comparison to Other Languages**:
- **Rust**: Similar to `Err(e) => return Err(e)` in match expressions
- **Swift**: Unlike Swift's implicit propagation with `try`
- **Java**: Unlike Java's catch-all `catch (Exception e)` which absorbs errors
- **Go**: Similar philosophy to checking `if err != nil { return err }`

## Design Decisions

### ✅ 1. Scope Semantics [RESOLVED]
**Question**: Does the catch block apply to the entire scope below it, or only to specific statements?

**Decision**: Applies to entire scope after the catch block

**Rationale**: Simplifies reasoning about error handling boundaries - the catch applies to everything below it in the same scope

---

### ✅ 2. Error Inference Rules [RESOLVED]
**Question**: How are error types inferred?

**Decision**: Analyze all function calls in every scope to determine which errors can be thrown

**Rationale**: 
- Provides maximum convenience - no need to explicitly annotate error types
- Compiler performs control flow analysis to determine all possible error types
- Functions must still be annotated with what they throw, but callers don't need to repeat this information

**Implementation Note**: Requires sophisticated static analysis to track error propagation through the call graph

---

### ✅ 3. Unhandled Errors [RESOLVED]
**Question**: What happens if an error type is not caught?

**Decision**: Implicitly propagated via automatic wildcard desugaring

**How It Works**:
The compiler **automatically adds** an implicit wildcard to every catch block that propagates unhandled errors. Developers only need to write explicit wildcards when they want to inspect/log errors before re-throwing.

**Example**:
```baml
// What you write
function Foo() -> Bar {
   catch {
      _: MyError => { return Bar.default() }
      // No explicit wildcard needed
   }
   // code that might throw MyError and OtherError
}

// What the compiler generates (desugared)
function Foo() -> Bar {
   catch {
      _: MyError => { return Bar.default() }
      __implicit__ => { throws __implicit__ }  // Auto-added by compiler
   }
   // code
}

// When you want to log/inspect unhandled errors
function Bar() -> Baz {
   catch {
      _: KnownError => { return fallback() }
      other => {  // Explicit wildcard overrides implicit one
         log.error("Unexpected error", other)
         throws other
      }
   }
}
```

**Rationale**:

- **Simplicity**: No need to write boilerplate wildcard in every catch block
- **Safety**: All errors are always either handled or propagated (no silent failures)
- **Future-Proof**: Handles dynamic errors from features like `eval` gracefully
- **Flexibility**: Developers can override with explicit wildcards when needed
- **No Compile Errors**: Since wildcards are implicit, code never fails due to "unhandled error"

---

### ✅ 4. Multiple Catches in Nested Scopes [RESOLVED]
**Question**: How do catches interact when scopes are nested?

**Decision**: Inner catches trigger first and can re-throw to outer scopes

**Example**:
```baml
function Foo() -> Bar {
   catch {
      _: MyError => { return Bar.default() }  // Outer catch
      other => { throws other }
   }
   
   if (condition) {
      catch {
         _: MyError => { return Bar.new(...) }  // Inner catch handles first
         other => { throws other }  // Re-throws to outer catch
      }
      // MyError thrown here goes to inner catch first
   }
}
```

**Rationale**: 

- Inner scopes have more specific context, so should handle errors first
- Catch handlers can use `return` to provide a value or `throws` to propagate
- Provides flexibility for both error recovery and propagation

---

### ✅ 5. Re-throwing and Error Propagation [RESOLVED]
**Question**: How do you propagate errors to callers?

**Decision**: Use explicit `throws` keyword with named wildcards

**Example**:
```baml
function Foo() -> Bar {
   catch {
      _: MyError => { return Bar.default() }
      // Named wildcard captures unhandled errors
      other => { throws other }
   }
   // code that might throw various errors
}
```

**Rationale**:

- Named wildcard pattern (`other => { throws other }`) makes propagation explicit
- Allows inspection of the error before re-throwing if needed
- Clear syntax for "handle specific errors, propagate the rest"

---

### ✅ 6. Variable Capture and Mutation [RESOLVED]
**Question**: What variables can the catch block access and modify?

**Decision**: Catch blocks can access all variables declared in **outer scopes** (and function parameters).

**Example**:
```baml
function Foo(param: T) -> Bar {
   let x = 10
   
   // Create a new scope to capture 'x'
   {
      catch {
         // ✅ Can access param (function parameter)
         // ✅ Can access x (declared in outer scope)
         return Bar.new(param, x)
      }
      // Code that uses x and might throw
      risky_op(x)
   }
}
```

**Rationale**:

- Makes error handling context-aware - handlers can use available state
- For function-level catches: access to function parameters
- For scope-level catches: access to variables declared in parent scopes
- Follows natural scoping rules while enforcing the "catch-at-top" constraint

---

### ✅ 7. Return Value Handling [RESOLVED]
**Question**: How does the catch block provide return values?

**Decision**: Catch handlers can use `return` to provide the function's return type, or `throws` to propagate

**Example**:
```baml
function Foo() -> Bar {
   catch {
      _: MyError => { return Bar.default() }  // Provide return value
      _: OtherError => { throws OtherError() }  // Re-throw
      other => { throws other }  // Propagate unhandled errors
   }
   // code that returns Bar
}
```

**Rationale**:

- Handlers have two options: recover (return) or propagate (throws)
- No need for Result/Optional wrapper types
- Clear and explicit control flow

---

### 8. Order of Error Handlers
**Question**: Does the order of error handlers in the catch block matter?

**Example**:
```baml
catch {
   _: SpecificError => { .. }
   _: GeneralError => { .. }  // Would this catch SpecificError if it extends GeneralError?
}
```

**Options**:

- **Option A**: First match wins (like switch/match)
- **Option B**: Must be mutually exclusive (compile error if ambiguous)
- **Option C**: Most specific match wins (prioritize by inheritance hierarchy)

---

### ✅ 9. Wildcards and Default Cases [RESOLVED]
**Question**: Should there be a way to catch "any error"?

**Decision**: Support named wildcards for catching unhandled errors

**Example**:
```baml
catch {
   _: MyError => { return Bar.default() }
   other => { 
      // 'other' is a named wildcard that captures any unhandled error
      log(other)
      throws other 
   }
}
```

**Rationale**:

- Named wildcards (e.g., `other`) provide access to the error instance
- Allows inspection/logging before re-throwing
- Makes it clear that unhandled errors exist and are being propagated
- More explicit than implicit propagation

---

### ✅ 10. Error Data Access [RESOLVED]
**Question**: How do you access the error instance/data in the handler?

**Decision**: Use pattern matching syntax (like Rust)

**Examples**:
```baml
catch {
   _: MyError => { .. }  // No access to error instance
   e: MyError => { .. }  // Access via parameter binding
   MyError { code, msg } => { .. }  // Destructure error fields
}
```

**Rationale**:
- Familiar to developers from Rust and other pattern-matching languages
- Supports both simple binding and destructuring
- Flexible: can choose to ignore, bind, or destructure based on needs
- Type-safe: compiler ensures destructured fields exist on the error type

---

### ✅ 11. Async/Await Interaction [RESOLVED]
**Question**: How does this work with async functions?

**Decision**: Top-level `main` function is responsible for catching all exceptions and returning a safe value. Runtime can provide implicit exception handler.

**Example**:
```baml
async function Foo() -> Bar {
   catch {
      _: NetworkError => { return Bar.default() }
   }
   await someAsyncCall()  // Can be caught by the catch block
}

// Top-level main function catches all uncaught exceptions
function main() {
   catch {
      other => { 
         print(other)
         return 1  // Safe exit code
      }
   }
   // ... application logic
}
```

**Rationale**:
- Async errors propagate through await points like synchronous errors
- Top-level `main` acts as final safety net for uncaught exceptions
- Runtime can provide implicit handler for exceptions that escape main
- Consistent behavior between sync and async code
- Prevents unhandled promise rejections

**Implementation Details**:
- Catch blocks work the same in async and sync functions
- Await expressions can throw errors that are caught by enclosing catch blocks
- Runtime-level implicit handler provides default behavior (e.g., log and exit) for truly uncaught exceptions

---

### ✅ 12. Compiler Guarantees [RESOLVED]
**Question**: What compile-time guarantees are provided?

**Decision**: Default to optional handling with implicit propagation, but support a **strict mode** for exhaustive checking of known errors.

**Context**: 
Since every catch block includes an implicit forwarder (`__implicit__ => { throws __implicit__ }`), strict exhaustiveness is not required for runtime safety—unhandled errors simply propagate. However, developers often want to ensure they haven't accidentally omitted a known error case.

**Syntax Addition**: `catch(strict)`

**Example**:
```baml
function Foo() -> Bar {
   // strict: Compiler ERROR if 'MyError' is reachable but not handled below
   catch(strict) {
      _: MyError => { return Bar.default() }
      // Implicit forwarder is STILL added for unknown/future errors
   }
   
   // code that throws MyError
}
```

**Rationale**:

- **Default Behavior**: "Loose" checking. Minimal friction. Unhandled known errors are propagated via the implicit forwarder.
- **Strict Mode**: `catch(strict)` enforces that all *known* error types reachable in the scope are explicitly handled.
- **Safety**: The implicit forwarder remains in *both* modes to safely propagate dynamic errors or errors from library updates that weren't known at compile time, preventing crashes while allowing strict checking of what *is* known.

---

### ✅ 13. Explicit Error Type Declarations [RESOLVED]
**Question**: Should error types be explicitly declared in function signatures (e.g. `throws [A, B]`)?

**Decision**: **No**. Rely entirely on inference.

**Rationale**:

- **Diff Management**: Renaming an error or adding a new exception would cause massive diffs if every function signature needed updating.
- **Agent-Friendly**: Explicit declarations are hard for agents to maintain and "super ugly" when iterating.
- **Inference**: Inference provides the necessary information without the boilerplate.

---

### ✅ 14. Placement and Frequency [RESOLVED]
**Question**: Can catch blocks appear anywhere in a scope, or multiple times?

**Decision**: **Strictly one** catch block per scope, located **only at the top**.

**Example**:
```baml
function Foo() -> int {
   // ✅ Valid: Top of function scope
   catch { 
      _: MyError => { return 0 }
   }

   // ❌ Invalid: Catch in middle of scope
   // catch { ... } 

   if (condition) {
      // ✅ Valid: Top of inner scope
      catch {
         _: MyError => { return 1 } // Returns from the block (which returns from function)
      }
      
      // ❌ Invalid: Multiple catches
      // catch { ... }

      throw MyError()
   }

   // Block return example
   let x = {
      // ✅ Valid: Top of block scope
      catch { 
         _: MyError => { return 10 } // Returns 10 from the FUNCTION
         _: MyError2 => { 10 } // Returns 10 from the BLOCK (binds x = 10)
      }
      fallible_op() // returns int
   }
}
```

**Rationale**:

- **Scope-Based Returns**: BAML follows Rust-like scoping where scopes (blocks) can return values. Placing the catch at the top respects these semantics, allowing the catch to handle both scope-returns and function-returns cleanly.
- **Simplicity**: "Only a single catch allowed" prevents confusion about which catch handles which statement.
- **Consistency**: Enforces a uniform structure across the codebase.

## Implementation Considerations

### Compiler Complexity
- Need to infer all possible error types in a scope
- Must track error propagation across function calls
- Requires sophisticated control flow analysis

### Runtime Overhead
- Likely similar to traditional try-catch
- May require special stack frame setup
- Error matching/dispatching cost

### IDE Support
- Need to show which code can throw which errors
- Highlight unhandled error types
- Suggest handlers when new error-prone code is added

## Next Steps

1. **Decision Required**: Resolve open design decisions (especially #1-4 are critical)
2. **Prototype**: Implement a minimal version to test ergonomics
3. **User Testing**: Get feedback from BAML users (and AI agents) on the syntax
4. **Comparison**: Create side-by-side examples with traditional try-catch to measure diff size
5. **Formalize**: Write formal syntax and semantics specification
6. **Tooling**: Consider how IDEs and AI coding assistants will interact with this syntax

## Summary

The scoped-catch syntax offers a novel approach to error handling that prioritizes **minimal diff overhead** and **clear variable scoping**. Its main innovation is placing error handlers at the top of scopes rather than wrapping code. This is particularly valuable for AI-assisted coding where small changes shouldn't create large diffs.

However, this approach introduces **unusual control flow** and requires resolution of significant design decisions around error inference, propagation, and scope semantics. The success of this proposal depends on carefully balancing convenience with type safety and predictability.
