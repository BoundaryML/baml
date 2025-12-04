# Design: Handling Primitive and Arbitrary Throw Types

We are exploring how to handle `throw` with arbitrary values (primitives, structs) while preserving metadata like stack traces.

## The Problem

If a user throws a primitive string:
```baml
throw "Something went wrong"
```

We need a way to:

1.  Capture the stack trace.
2.  Allow the user to access that stack trace in the catch block.
3.  Allow the user to access the original string value.

## Idea 1: Universal `Exception<T>` Wrapper

**Vaibhav's recommendation**

In this model, **every** caught value is wrapped in an `Exception<T>` envelope. The pattern match variable binds to this envelope, not the raw value.

### Syntax

```baml
catch {
   // 's' is Exception<string>, NOT string
   s: string => { 
      log("Error: " + s.value) // Access raw value
      log("Stack: " + s.stack) // Access metadata
   }

   // 'e' is Exception<MyErrorType>
   e: MyErrorType => {
      log("Error ID: " + e.value.id)
      log("Stack: " + e.stack)
   }
}
```

### Destructuring Sugar

To make this ergonomic for structural types, destructuring syntax is syntactic sugar for accessing `.value`.

```baml
// This syntax:
catch {
   MyErrorType { id, message } => { ... }
}

// Desugars to:
catch {
   temp: MyErrorType => {
      let { id, message } = temp.value
      ...
   }
}
```

### Pros/Cons
- **Pro**: Consistent. Everything has a stack trace accessible in the same way.
- **Pro**: Works for primitives without needing special "Exception" matching syntax.
- **Con**: Slightly more verbose for simple cases (`s.value` vs `s`).

## Idea 2: Restrict to `Error` Interface

NOTE: IGNORE SYNTAX FOR HOW ERROR INTERFACE IS DEFINED. THIS IS JUST FOR ILLUSTRATION.

Alternatively, we could disallow throwing arbitrary types and require all thrown values to implement a well-defined interface.

```baml
interface Error {
   message string
   stack StackTrace?
}

class MyError implements Error { ... }
```

### Implications
- `throw "string"` would be illegal. You must throw `new Error("string")`.
- `throw 123` would be illegal.
- Users must define proper error classes.

### Pros/Cons
- **Pro**: Enforces good practices (typed errors).
- **Pro**: Guaranteed structure for all errors.
- **Con**: Higher friction for prototyping (can't just `throw "fail"`).
- **Con**: Less flexible than "throw anything".

## Open Questions

1.  Is the verbosity of `s.value` in Idea 1 acceptable?
> IMO, verbosity isn't a deal breaker because models are writing the code anyway
2.  Is the friction of Idea 2 acceptable for a language designed for AI prototyping?
3.  Should we support a hybrid? (e.g., primitives are auto-wrapped in a standard `Error` class, but structs must implement `Error`?)
> No
