# Scoped Catch vs. Go 2 Proposals

This document explores the relationship between BAML's **Scoped Catch** and the rejected **Go 2 Error Handling Proposal** (`check`/`handle`).

## The Go 2 Proposal (2018)

In 2018, the Go team proposed a new error handling design to address the verbosity of `if err != nil`.

**Proposed Syntax (`check` & `handle`)**:
```go
func CopyFile(src, dst string) error {
    handle err {
        return fmt.Errorf("copy %s %s: %v", src, dst, err)
    }

    r := check os.Open(src)
    defer r.Close()

    w := check os.Create(dst)
    handle err {
        w.Close()
        os.Remove(dst) // Clean up partial file on error
    }

    check io.Copy(w, r)
    check w.Close()
    return nil
}
```

The proposal was **rejected** due to community feedback. Below, we analyze specific critiques and how BAML addresses them.

## 1. The "Inscrutable Chain" vs. Fixed Placement

**Go 2 Critique**: The `handle` blocks could appear anywhere in a function, and the rules for which handler caught which error (lexical scoping vs. runtime execution) were confusing. Liam Breck called this the "inscrutable chain."

> "The steps taken on bail-out can be spread across a function and are not labeled... For the following example, cover the comments column and see how it feels…"
> — **Liam Breck**, *[Golang, How dare you handle my checks!](https://medium.com/@mnmnotmail/golang-how-dare-you-handle-my-checks-d5485f991289)*

```go
// Liam Breck's example of confusing control flow
func f() error {
   handle err { return ... }           // finally this
   if ... {
      handle err { ... }               // not that
      for ... {
         handle err { ... }            // nor that
         ...
      }
   }
   handle err { ... }                  // secondly this
   ...
}
```

**BAML Solution**: BAML enforces a strict **"Last Statement Only"** rule. The `catch` block *must* be attached to the end of the scope.

- **Benefit**: This acts as a predictable "trailer" for the scope. There is no ambiguity about where the handler is—it's always at the end. It functions mentally like a `try` block without the indentation.

## 2. Loss of Local Context

**Go 2 Critique**: Nate Finch argued that `check` removed the physical space in code to add context to a specific error (e.g., distinguishing "A failed" from "B failed").

> "With check, that space in the code doesn’t exist. There’s a barrier to making that code handle errors better... Most of the time I want to add information about one specific error case."
> — **Nate Finch**, *[Handle and Check - Let's Not](https://npf.io/2018/09/check-and-handle/)*

**BAML Solution**: BAML supports **Expression Blocks** with catch alongside Scope-level Catch.

- **Benefit**: When you need specific context for a single call, you use an expression block. When you want broad resilience for a block of logic, you use the scope-level syntax. You have the best of both worlds.

```baml
// BAML allows local context when needed
let user = {
    FetchUser(id)
} catch {
    _: NotFound => { return null } // Specific handling for this call
}
```

## 3. "Spooky Action at a Distance"

**Go 2 Critique**: A `check` at the bottom of a function jumping to a `handle` at the top breaks the principle of locality.

**BAML Trade-off**: This critique applies less to BAML's trailing catch, as the handler is at the bottom (closer to where execution falls through).

- **Mitigation**: BAML is a DSL for AI pipelines, where the "happy path" is often a linear sequence of operations. The value of **"Additive Resilience"** (adding error handling without refactoring/indenting) for AI agents and prototyping outweighs the control flow jump.
- **Alternative**: For complex control flow where this jump is confusing, developers can fall back to using nested blocks `{ ... } catch { ... }` or expression blocks to keep handling local.

## 4. Specificity to Error Type

**Go 2 Critique**: `check` was hardcoded to the `error` interface and couldn't handle other success/failure patterns (like boolean flags).

**BAML Context**: BAML is designing a dedicated exception system, not trying to retrofit an existing value-based error system. The mechanism is explicitly for *exceptions*, so this specificity is a feature, not a bug.

## 5. Ergonomic Differences

Beyond control flow, BAML makes specific ergonomic choices that differ from the Go 2 proposal:

### No Call-Site Keywords
**Go 2**: Required `check` at every fallible call site.
```go
x := check foo() // Visual noise at every line
```
**BAML**: No keywords required at call sites for scope-level handling.
```baml
let x = foo() // Clean, "happy path" syntax
```
This reduces visual noise and makes it easier to prototype (you don't need to know if a function throws to call it, unless you're in strict mode).

### Familiar Keyword (`catch`)
**Go 2**: Introduced a new keyword `handle`, which felt foreign to many developers.
**BAML**: Reuses `catch`, a keyword familiar to almost every developer from Java, JS, Python, C++, etc. The semantics are slightly different (header vs. wrapper), but the *intent* is immediately recognizable.

### Pattern Matching vs. Variable Declaration
**Go 2**: `handle err` introduces a new variable `err` into the scope, which can shadow other variables or be shadowed itself.
```go
handle err { ... } // 'err' is now in scope
```
**BAML**: Uses pure pattern matching. No variable is introduced until you explicitly bind one in a match arm.
```baml
catch {
   // No variable 'e' exists here
   e: MyError => { ... } // 'e' exists only in this block
}
```
This prevents accidental shadowing and makes it clear exactly what data is available.

## Alternative Ideas from Go Community

The Go community proposed alternatives that are also relevant to BAML's design process.

### Assignment Syntax
Many users preferred an inline syntax:
```go
// From mcluseau's proposal
msg, check readErr := remote.Read()
```
BAML's expression block catch is spiritually similar to this, allowing handling at the assignment site.

### Named Handlers
Some proposed explicitly invoking handlers:
```go
check f() ? handlerName
```
BAML avoids this complexity by sticking to standard scoping rules (inner catches handle first).
