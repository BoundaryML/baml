# TypeScript Match Syntax

TypeScript does not have a native `match` expression like Rust or Scala. Instead, it relies on Discriminated Unions and `switch` statements (or `if` chains) to achieve similar type-safe behavior.

## Discriminated Unions

The core pattern in TypeScript is the "Discriminated Union" (also called Tagged Union).

```typescript
interface Circle {
  kind: "circle";
  radius: number;
}

interface Square {
  kind: "square";
  sideLength: number;
}

type Shape = Circle | Square;

function getArea(shape: Shape) {
  switch (shape.kind) {
    case "circle":
      // TypeScript knows shape is Circle here
      return Math.PI * shape.radius ** 2;
    case "square":
      // TypeScript knows shape is Square here
      return shape.sideLength ** 2;
  }
}
```

## Exhaustiveness Checking

You can enforce exhaustiveness using the `never` type.

```typescript
function getArea(shape: Shape) {
  switch (shape.kind) {
    case "circle":
      return Math.PI * shape.radius ** 2;
    case "square":
      return shape.sideLength ** 2;
    default:
      // This line will cause a compile error if a new Shape is added but not handled
      const _exhaustiveCheck: never = shape;
      return _exhaustiveCheck;
  }
}
```

## Libraries

Because native support is verbose (statement-based), libraries like `ts-pattern` are popular.

```typescript
import { match } from 'ts-pattern';

const result = match(shape)
  .with({ kind: 'circle' }, (c) => Math.PI * c.radius ** 2)
  .with({ kind: 'square' }, (s) => s.sideLength ** 2)
  .exhaustive();
```

## Proposal

There is a TC39 proposal for Pattern Matching in JavaScript/TypeScript, which would look like:

```typescript
// Proposed syntax (not yet available)
const result = match (shape) {
  when ({ kind: 'circle', radius }) -> Math.PI * radius ** 2,
  when ({ kind: 'square', sideLength }) -> sideLength ** 2
}
```
