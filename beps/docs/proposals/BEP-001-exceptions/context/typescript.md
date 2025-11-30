# Deep Dive: TypeScript / JavaScript Error Handling

## Core Philosophy: Dynamic & Permissive
JavaScript (and thus TypeScript) allows **throwing any value**.
```typescript
throw new Error("oops");
throw "oops"; // Legal
throw null;   // Legal
```
TypeScript tries to tame this, but because the underlying runtime is dynamic, it cannot statically guarantee what is thrown.

## Developer Experience (DX)

### The `unknown` Catch Block
In modern TypeScript (`useUnknownInCatchVariables`), caught errors are `unknown`.
```typescript
try {
    risky();
} catch (e: unknown) {
    // You MUST check the type
    if (e instanceof Error) {
        console.log(e.message);
    } else {
        console.log("Someone threw a non-Error:", e);
    }
}
```
**DX Friction**: This forces defensive coding. You can't just access `e.message` without a guard.

### Async Error Handling
Promises have their own error channel.
```typescript
const [result] = await Promise.allSettled([req1, req2]);
if (result.status === "rejected") {
    console.error(result.reason); // reason is 'any' or 'unknown'
}
```

### The "Result" Pattern (Community Solution)
Because native exceptions are untyped, many TS teams adopt a Rust-like `Result` type via libraries like `neverthrow` or `fp-ts`.

```typescript
import { ok, err, Result } from 'neverthrow';

function divide(a: number, b: number): Result<number, Error> {
    if (b === 0) return err(new Error("Zero division"));
    return ok(a / b);
}

// Usage forces handling
const result = divide(10, 0);
if (result.isErr()) {
    // handle error
}
```

## Implementation Tradeoffs

### 1. Static Types vs. Runtime Reality
**Tradeoff**: TypeScript types are erased at runtime.

- **Problem**: You can declare `function foo(): void` but it can still throw. There is no `throws` signature in TS.
- **Consequence**: Callers are never forced to handle errors. Unhandled Promise rejections are a common source of crashes (or silent failures in older Node versions).

### 2. Structural Typing vs. Nominal Errors
**Tradeoff**: `instanceof` checks the prototype chain (nominal).

- **Problem**: If you have two error classes with the same structure, `instanceof` distinguishes them. But if you serialize/deserialize an error (e.g., from a worker or API), the prototype chain is lost, and `instanceof` fails.
- **Workaround**: Structural checks (`if ('code' in e && e.code === 'ENOENT')`).

## Summary
TypeScript suffers from the "worst of both worlds" in error handling: **no static guarantees** (like Java/Rust) but **required type guards** (like strict languages). The community often bypasses native exceptions in favor of `Result` types to regain control.
