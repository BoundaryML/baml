# Alternative Error Handling Ideas

This document explores "wild" alternatives to the Scoped Catch (Header) proposal, specifically addressing the feedback about **Control Flow Inversion** ("handle first, compute after") and **Variable Access**.

## Idea 1: The "On Error" Statement (Imperative Registration)

Instead of a static block at the top, `on_error` is an imperative statement that registers a handler for the *current scope* from that line forward.

### Syntax
```baml
function Process(id: string) {
    // 1. Initial handler
    on_error {
        _: NotFound => return null
    }

    let user = db.getUser(id)
    
    // 2. Register a new handler that HAS ACCESS to 'user'
    on_error {
        e => {
            log.error("Failed processing user", { user: user, error: e })
            throw e
        }
    }

    // This risky operation is covered by the second handler
    risky_operation(user)
}
```

### Pros
*   **Solves the "Data" Problem**: You can define handlers *after* you have the data they need.
*   **Linear Flow**: You read top-to-bottom.
*   **Additive**: You can drop an `on_error` line anywhere without re-indenting.

### Cons
*   **"Inscrutable Chain"**: If you have multiple `on_error` statements, it can be hard to track which one is active. (Mitigation: `on_error` could stack or replace? "Defer" semantics?)

---

## Idea 2: Postfix `catch` (Expression Level)

Attach error handling directly to the expression that might fail. This is similar to Ruby's `rescue` modifier or Perl's `or die`.

### Syntax
```baml
function Process(text: string) {
    // Simple fallback
    let data = extract(text) catch { return null }

    // Complex handling with block
    let result = risky_call() catch {
        e: Timeout => retry()
        e: AuthError => raise e
    }
}
```

### Pros
*   **Extremely Local**: The handler is right next to the failure.
*   **No Indentation**: Happy path stays on the left.
*   **Familiar**: Similar to `.catch()` promises in JS.

### Cons
*   **Verbosity**: If you have 10 lines of risky code, you need 10 `catch` clauses (or wrap them in a block, re-introducing nesting).

---

## Idea 3: Trailing `rescue` (Implicit Try)

Allow a `rescue` / `catch` block at the **end** of a function or scope. The entire scope is implicitly treated as the "try" block.

### Syntax
```baml
function Example() {
    let x = 1
    do_thing()
    
    return x
} catch {
    // Handles errors from the ENTIRE function body above
    e: Error => {
        log(x) // ⚠️ Scoping issue: is 'x' available?
        return 0
    }
}
```

### Pros
*   **Natural Control Flow**: "Do this. If that failed, do this."
*   **Clean Happy Path**: The main logic is unindented at the top.

### Cons
*   **Variable Scoping**: Accessing variables defined in the body is tricky (they might not be initialized).
*   **Distance**: The handler might be far away from the error source.

---

## Idea 4: Guard Clauses / `let else`

Focus on "ensuring success" rather than "catching failure". Inspired by Swift `guard` and Rust `let else`.

### Syntax
```baml
function Example() {
    // "Ensure this succeeds, otherwise run this block"
    guard let data = risky_call() else {
        return null
    }

    // 'data' is safe to use here
    process(data)
}
```

### Pros
*   **Happy Path Focus**: Emphasizes the successful data flow.
*   **Early Return**: Encourages handling errors immediately and returning.

### Cons
*   **Limited Pattern Matching**: Usually only handles "success vs failure", harder to match specific error types (Timeout vs Auth) without more syntax.

---

## Idea 5: Decorators / Attributes

Move error handling completely out of the function body, into metadata.

### Syntax
```baml
@[Catch(TimeoutError, return: null)]
@[Catch(AuthError, strategy: "retry", attempts: 3)]
function Example() {
    risky_call()
}
```

### Pros
*   **Zero Clutter**: The function body is pure logic.
*   **Reusable Policies**: Can define standard error policies.

### Cons
*   **Rigid**: Hard to do custom logic (logging specific variables, complex recovery).
*   **"Magic"**: Hides control flow.

---

## Idea 6: The "Recover" Expression

A dedicated block for attempting code and recovering.

### Syntax
```baml
function Example() {
    let val = attempt {
        risky_step_1()
        risky_step_2()
    } recover {
        _: Timeout => null
        e => throw e
    }
}
```

### Pros
*   **Explicit Scope**: Clear what is covered.
*   **Expression-oriented**: Returns a value.

### Cons
*   **Indentation**: Back to the `try/catch` nesting problem.
