# Background: The Error Handling Landscape

Before diving into BAML's exception handling design, let's establish the problem space. Error handling is one of those topics where every language makes different trade-offs, and understanding those trade-offs helps explain why we chose the path we did.

## The Purpose of Error Handling

At its core, error handling serves two distinct purposes:

1. **Recoverable Failures**: Expected runtime conditions that your program should handle gracefully. Network timeouts, invalid user input, file not found. Your code should catch these, log them, retry, or fall back to alternatives.

2. **Bugs**: Programmer mistakes. Array index out of bounds, null pointer dereference, failed assertion. When these happen, the right thing to do is crash immediately with a clear stack trace so you can fix the code.

Most languages blur this distinction. 

* TypeScript lets you catch everything, but provides no language-level way to distinguish recoverable errors from bugs.
* Java forces you to handle checked exceptions, but unchecked exceptions (bugs like `NullPointerException`) can still crash your program unhandled.
* Rust makes the distinction explicit (`Result<T, E>` for recoverable errors, `panic!` for bugs), but at the cost of verbosity.

Different failure modes need different handling strategies. Recoverable errors need graceful degradation. Bugs need to crash early with clear diagnostics.

## Landscape Survey

Let's look at how different languages approach this problem.

### Java: Checked vs Unchecked Exceptions

Java tried something interesting with checked exceptions. The idea was: if your method can fail in a recoverable way, you have to declare that in the signature. The compiler then forces every caller to either handle it or pass it up the chain.

```java
// Checked exception - must be declared
public void readFile() throws IOException, FileNotFoundException {
    // ...
}

// Unchecked exception - no declaration needed
public void divide(int a, int b) {
    if (b == 0) {
        throw new IllegalArgumentException("Division by zero"); // RuntimeException
    }
    // ...
}

public void main() {
    try {
        readFile(); // Must handle IOException
    } catch (FileNotFoundException e) {
        // Handle specific case
    } catch (IOException e) {
        // Handle general case
    }
    
    divide(10, 0); // No try-catch required, but will throw at runtime
}
```

On paper, this sounds great—you can look at a function signature and know exactly what can go wrong. Self-documenting APIs.

In practice? It's a nightmare. Say you're six layers deep in your call stack and you realize your database layer needs to throw a new exception type. Now you have to update the signature of every function in that chain. Your clean API just got polluted with implementation details bubbling up from the depths.

It gets worse with lambdas. Java's `Function<T, R>` interface doesn't declare any checked exceptions, so you literally can't throw one from a lambda. Everyone just wraps their exceptions in `RuntimeException` and rethrows—which defeats the entire point of checked exceptions.

The Java ecosystem has basically given up on this. Spring, Hibernate, and most modern frameworks use unchecked `RuntimeException` subclasses exclusively. So you end up with this weird split: the standard library uses checked exceptions, but everything else uses unchecked ones. The "safety" checked exceptions were supposed to provide? Most production Java code doesn't have it.

### Rust: Result vs Panic

Rust took a different approach: errors are just values. A function that can fail returns `Result<T, E>`—either you get your value, or you get an error. No magic, no hidden control flow.

```rust
fn read_config() -> Result<Config, io::Error> {
    let mut file = File::open("config.toml")?; // ? propagates errors
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(parse(contents))
}
```

That `?` operator is doing the heavy lifting here—it's basically "unwrap this, or return the error to my caller." You get the safety of explicit error handling without writing `match` statements everywhere.

The nice thing about this model is that it's honest. You can look at a function signature and see exactly what errors it can produce. The compiler won't let you ignore a `Result`—you have to deal with it somehow. And because errors are just values, there's no runtime overhead; the compiler optimizes it all away.

But here's the friction: every function that can fail needs `Result` in its return type. And then you run into the error type alignment problem—if function A returns `Result<_, IoError>` and function B returns `Result<_, ParseError>`, composing them means you need some common error type. You end up with error enums, or `Box<dyn Error>`, or crates like `anyhow` and `thiserror` to paper over the boilerplate.

For systems programming and libraries, this is exactly what you want. For a quick script where you just want to "try this thing and bail if it fails"? The ceremony can feel like overkill.

### TypeScript: Unchecked Exceptions

TypeScript just inherits JavaScript's exception model: throw whatever you want, catch it if you feel like it. The type system pretends exceptions don't exist.

```typescript
function risky(): void {
    throw new Error("oops");
    // TypeScript doesn't know this can throw
}

try {
    risky();
} catch (e: unknown) {
    // Modern TS forces 'unknown' type
    if (e instanceof Error) {
        console.log(e.message);
    }
}
```

This is great for prototyping—zero ceremony, just write code. But you pay for it in production. That function signature says `(): void`, not "this might blow up." There's no way to know a function can throw without reading its implementation (or hoping the docs are accurate).

The async story is even worse. Unhandled Promise rejections used to just silently disappear; now they crash your Node process. Either way, it's a common source of bugs because nothing in the type system reminds you to handle them.

Some teams reach for libraries like `neverthrow` or `fp-ts` to get Rust-style `Result` types in TypeScript. You get exhaustiveness checking, explicit error types, the whole deal. But it's a big shift in how you write code, and most teams end up with a mix—`Result` for the critical paths, regular exceptions everywhere else. Not ideal, but it's the pragmatic choice.

### Effect.ts: Functional Error Handling for TypeScript

Effect.ts is the "we want Rust-style error handling but we're stuck in TypeScript" solution. It's a library that gives you typed errors, exhaustiveness checking, and composable error handling—but as a framework you opt into, not a language feature.

The core idea: instead of functions that throw, you have functions that return `Effect<T, E, R>`—a computation that produces `T`, might fail with `E`, and needs resources `R`.

```typescript
import { Effect, pipe } from "effect";

// Define your error types
class FileReadError {
  constructor(public message: string) {}
}

class ParseError {
  constructor(public message: string) {}
}

type AppError = FileReadError | ParseError;

// Functions return Effect instead of throwing
function readFile(path: string): Effect.Effect<string, FileReadError> {
  return Effect.tryPromise({
    try: () => fs.promises.readFile(path, "utf-8"),
    catch: (e) => new FileReadError(String(e))
  });
}

function parseJson<T>(json: string): Effect.Effect<T, ParseError> {
  return Effect.try({
    try: () => JSON.parse(json),
    catch: (e) => new ParseError(String(e))
  });
}

// Compose them together
const program = pipe(
  readFile("config.json"),
  Effect.flatMap(parseJson<Config>), // Chain operations
  Effect.catchAll((error: AppError) => {
    // Exhaustiveness checking ensures you handle all error types
    if (error instanceof FileReadError) {
      return Effect.succeed(defaultConfig);
    } else {
      return Effect.fail(error); // Re-throw ParseError
    }
  })
);
```

When it works, it's beautiful. You get type-safe errors, the compiler yells at you if you forget to handle a case, and everything composes nicely. If you've ever wanted TypeScript to be more like Rust, this is pretty close.

But let's be real about what you're signing up for. This is a completely different way of writing code. Every operation that can fail gets wrapped in `Effect`. You're chaining `flatMap` and `pipe` instead of writing imperative code. Your team needs to learn a new mental model, and anyone reading your code needs to understand Effect.

Also worth noting: Effect only handles the errors you explicitly model. If you hit a null pointer or an array bounds error, that's still a regular JavaScript exception that'll crash your program. Effect is for recoverable errors you anticipate, not bugs.

Most teams find Effect is too heavy for application code. Where it shines is library code where you really want strong contracts about what can fail and how. For everything else, the learning curve and boilerplate usually aren't worth it.

## Error Handling in LLM Systems

Here's where LLM systems are fundamentally different from traditional software.

In normal code, exceptions are edge cases. You parse some JSON, it works. You make a network call, it usually succeeds. Errors are exceptional—that's why we call them exceptions.

In code that uses LLMs, errors handling is a must:

- The LLM **might** refuse your request because content policy
- The LLM **might** return structurally valid JSON that's semantically garbage
- The LLM **might** timeout because the model is overloaded
- The LLM **might** return something that parses fine but violates your business logic

Production systems regularly:

- Retry requests with exponential backoff
- Switch between models when one fails
- Fall back to heuristics when parsing fails

Given that errors are expected and frequent in LLM systems, the error handling mechanism must satisfy the following requirements:

## Requirements

1. **Works in Declarative Functions**: Error handling must work seamlessly with declarative LLM function definitions (functions with `client` and `prompt` declarations).

2. **No Structural Changes**: Adding error handling should not require structural refactoring of existing code. The happy path should remain unchanged.

3. **Gradual Escalation**: Moving from prototype code to production code should be easy. You should be able to start with simple, error-free code and incrementally add error handling without major refactoring.

4. **Safety Guarantees**: Developers must be able to reason about the safety guarantees of a function—what errors it handles, what errors it propagates, and what bugs it allows to crash.

5. **External Consistency**: Similarity to exception handling in other languages (particularly TypeScript/JavaScript) to reduce learning curve.

6. **Internal Consistency**: The same error handling mechanism should work for both imperative functions (with control flow) and declarative functions (with LLM config).

7. **Tooling Preservation**: The design must preserve IDE features like prompt previews and auto-completion.
