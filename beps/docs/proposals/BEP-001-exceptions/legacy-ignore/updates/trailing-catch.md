# Trailing Catch (Implicit Try)

This proposal introduces **Trailing Catch**, a modification to the Scoped Catch syntax where the `catch` block is placed **after** the scope instead of at the beginning.

## Motivation

Feedback on the initial "Header Catch" (Top-of-scope) proposal highlighted two significant ergonomic issues:

1.  **Control Flow Inversion**: Reading "Handle Error X" before seeing the code that causes it feels unnatural.
    > "I don't like the control flow 'feeling' I get from this... This is the opposite: handle first (what?), compute after." — *Antonio*
2.  **Variable Scoping Friction**: A top-level catch block cannot access variables defined inside the function, even if those variables are initialized before the error occurs. This forces developers to "hoist" variables or create artificial scopes just to make data available to the error handler.
    > "I cannot use variables that would normally be available in catch from the outer scope... so I need to adjust my type system to carry all the stuff I need inside the catch." — *Antonio*

## The Proposal

Instead of placing `catch` at the *start* of a scope, we place it at the *end*. The scope itself acts as an implicit `try` block.

### Syntax

#### 1. Function-Level Catch

```baml
function Extract(text: string) -> Resume {
    client "openai/gpt-4o"
    prompt #"Extract resume from: {{ text }}"#
} catch {
    // Handles errors from the ENTIRE function body
    _: LlmError => { 
        return Resume { name: "Unknown", experience: [] } 
    }
}
```

#### 2. Block-Level Catch

```baml
function Process(id: string) {
    let user = db.getUser(id)
    
    // Create a scope for risky operations
    {
        risky_operation(user)
        another_risky_operation(user)
    } catch {
        // ✅ Can access 'user' because it was defined BEFORE the block
        e => {
            log.error("Failed processing user", { user: user, error: e })
        }
    }
}
```

### Scoping Rules

The `catch` block follows standard lexical scoping rules, treating the attached scope as a sibling.

1.  **Outer Scope Access**: The `catch` block has access to all variables defined **prior** to the scope it is attached to.
    *   For **Function-Level**: Access to all function parameters.
    *   For **Block-Level**: Access to all variables defined in the parent scope *before* the block started.
2.  **Inner Scope Isolation**: The `catch` block does **not** have access to variables defined *inside* the attached scope (because the scope execution was interrupted).

### Comparison

#### Scenario: Using Intermediate Data in Error Handling

**Problem**: We want to log `data` if `step2(data)` fails.

**Original Proposal (Top-Level Catch)**:
*   Fails because `catch` is at the top and can't see `data`.
*   Fix requires creating a nested block *and* putting catch at the top of *that*.

```baml
// ❌ Original Proposal (Awkward)
function Example() {
    // 1. Must create a block
    {
        // 2. Must put catch at the top
        catch {
             e => log(data) // ✅ Works, but reads backwards
        }
        step2(data)
    }
    // 3. Wait, where do I define 'data'? 
    // If I define it inside the block, catch can't see it.
    // If I define it outside, I have to separate declaration and usage.
    
    let data = step1() // ❌ Wait, this needs to be BEFORE the block for catch to see it
}
```

**New Proposal (Trailing Catch)**:
*   Natural linear flow. Define data, start block, handle error at end.

```baml
// ✅ Trailing Catch (Natural)
function Example() {
    let data = step1()
    
    {
        step2(data)
    } catch {
        e => log(data) // ✅ Works perfectly. 'data' is in outer scope.
    }
}
```

## Addressing the Feedback

| Feedback | Top-Level Catch | Trailing Catch |
| :--- | :--- | :--- |
| **"Control Flow Inversion"** | **Bad**: "Handle first, compute later" | **Good**: "Compute, then handle if failed" |
| **"Variable Access"** | **Bad**: Must hoist variables or nest awkwardly | **Good**: Access to all prior variables naturally |
| **"Nesting"** | **Medium**: Requires `catch { ... }` block inside scopes | **Good**: Uses standard `{ ... } catch { ... }` blocks |
| **"Diff Size"** | **Excellent**: Additive at top | **Good**: Additive at bottom (still no re-indenting of body) |

## Conclusion

Trailing Catch retains the primary benefit of the original proposal—**no indentation of the happy path**—while resolving the cognitive dissonance of "handling errors before they happen." It aligns BAML more closely with the mental model of `try/catch` (without the `try` keyword or indentation penalty) and solves the variable scoping issues by placing the handler lexically *after* the variable definitions it depends on.
