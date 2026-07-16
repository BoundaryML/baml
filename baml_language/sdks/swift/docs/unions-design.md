# Swift Bridge: Union Representation Decision

**Status:** accepted (2026-07-16) · replaces the Phase 3 named-enum design
**Applies to:** `sdkgen_swift` + `BamlBridge` · aligned with the cross-bridge C#/Java direction

## Decision

BAML unions are represented by **one reusable generic family** in the runtime library —
`BamlUnion2<T0, T1>`, `BamlUnion3<T0, T1, T2>`, … — instead of generating a named Swift
enum per structural union shape. **No synthesized public type names** (`IntOrString`,
`CardPaymentOrWirePayment`, …) are ever emitted.

```
int | string                 →  BamlUnion2<Swift.Int, Swift.String>
int | null | string          →  BamlUnion2<Swift.Int, Swift.String>?     (null strips to Optional)
int | string | MyType        →  BamlUnion3<Swift.Int, Swift.String, Baml.ns.MyType>
int | int                    →  Swift.Int                                (dedup + singleton collapse)
"draft" | "sent" | "paid"    →  Swift.String                             (literal-only collapse)
type Pay = Card | Wire       →  public typealias Pay = BamlUnion2<Card, Wire>
type RecList = int|RecList[] →  nominal `indirect enum RecList` (forced exception, see below)
```

The family is **written once, by hand, in `BamlBridge`** (like `Dictionary` in the Swift
stdlib): one declaration per arity up to a cap (initially 8), instantiated by generics.
A program with 100 distinct union shapes adds zero new type declarations.

## Why we changed

The named-enum design had four problems, confirmed by cross-bridge review:

1. **Every generated name is an API promise** — naming, collision suffixes, IntelliSense
   surface; ~1 public type per union expression.
2. **Placement instability**: the enum lived in the namespace of *first use* (alphabetical).
   Adding an unrelated BAML function could move a public type — silent source breakage.
3. **Nominal fragmentation** of a structural concept: two same-shaped unions were unrelated
   Swift types; alias and anonymous forms didn't interoperate.
4. **Decode was a heuristic where ground truth exists**: we discarded the wire's
   `union_variant_value.value_option_name` (which names the selected arm) and re-guessed by
   structural try-order. Same-shaped arms could silently mis-decode.

C# ships `BamlUnion<T0,T1>` (arity-overloaded name + implicit conversions); Java ships
`BamlUnion2/3/…` (sealed interfaces + `match`). Swift's spelling is the same family as
Java's — numbered arities, explicit construction — with Swift-native upgrades.

## The family type

```swift
public indirect enum BamlUnion2<T0, T1> {
    case t0(T0)
    case t1(T1)

    // Type-directed layer (insertion-stable):
    public init(_ value: T0)                       // arm chosen by argument type
    public init(_ value: T1)
    public func value<T>(as type: T.Type) -> T?    // std::get_if analog
    public func holds<T>(_ type: T.Type) -> Bool   // holds_alternative analog
    public var anyValue: Any { get }               // for `case let x as T` switches

    // Positional layer (exhaustive):
    public func match<R>(t0 onT0: (T0) throws -> R, t1 onT1: (T1) throws -> R) rethrows -> R
    public func match<R>(t0 onT0: (T0) async throws -> R, t1 onT1: (T1) async throws -> R) async rethrows -> R
    public var t0: T0? { get }
    public var t1: T1? { get }
}
// conditional Equatable / Hashable / Sendable / BamlEncodable / BamlDecodable
// BamlUnion3 … BamlUnion8 identical in pattern
```

- `indirect` → unions break struct-recursion cycles for free (the cycle-boxer skips them).
- Cases are **positional** (`t0`, `t1`, …) in **canonical BAML arm order**. The tag is not
  the wire name; wire identity lives in the codec layer.
- Swift enums have no default/zero value → the "default must be invalid" invariant is free.
- Duplicate projections (`T | string` with `T = string` → `BamlUnion2<String, String>`)
  stay distinguishable by tag; never `is`-check to identify an arm.

## Construction

No implicit conversions in Swift (C#-only trick), but leading-dot + target typing keeps
call sites terse — better than Java's factories, worse than C#:

```swift
try Baml.accept(value: .t0("hello"))
try Baml.accept(value: .t1(42))
let v: BamlUnion2<String, Int> = .t0("hello")   // both params inferred from target
```

**Stated cost:** the old literal-expressibility sugar (`flags: [7, "manual-review", true]`)
does not survive the generic model — "conform when exactly one arm is Int" is not
expressible in Swift's conformance system. Union values are constructed with explicit
cases: `[.t0(7), .t1("manual-review"), .t2(true)]`.

## Consumption

**`match` — the canonical cross-bridge API.** One required closure per arm (exhaustive by
signature), unified inferred result type, `rethrows`, labeled for readability:

```swift
let display = result.match(
    t0: { n in "int: \(n)" },
    t1: { s in "string: \(s)" },
    t2: { m in "mytype: \(m.v)" }
)
```

**Native `switch` — the Swift bonus.** Because the family is a real enum (what Java needs
sealed-interface ceremony for, and C# 14 can't express):

```swift
switch result {
case .t0(let n):  ...
case .t1(let s):  ...
case .t2(let m):  ...
}
```

**Accessors** for single-arm peeks: `result.t2?.v`.

## Wire contract (the shared invariants)

- **Encode:** the selected arm's value rides **bare** — no union wrapper inbound; the
  engine re-validates against the declared union.
- **Decode: active arm from canonical BAML identity, never inference.** Order:
  1. Match the wrapper's `value_option_name` against arm identities
     (`_bamlArmIdentity` hook: `"int"`, `"string"`, class/enum FQNs).
  2. Class-arm FQN match via the wire class name.
  3. Structural try-order in declared order — fallback only.
- Arm order is canonicalized from typed BAML identity at codegen time.
- The internal case tag (`t0`) never doubles as the wire name.

## Forced exceptions (language limits, not design splits)

- **Recursive union aliases** (`type RecList = int | RecList[]`): Swift `typealias` cannot
  self-reference, so these emit a nominal `indirect enum` under the **user's own name**
  (not synthesized) with the exact family surface (`t0/t1` cases, `match`, accessors).
  Java/C# need the same escape hatch.
- **Arity cap**: numbered arities up to `BamlUnion8` initially (fixture max is 4). A wider
  union is reported unsupported; raising the cap is an additive runtime-library change.
- **Literal-only unions** collapse to their base type (no raw-value enums — they were
  generated names). Compile-time literal safety is traded for the no-names invariant;
  the engine still validates values.

## Cross-language position

| Capability | C# | Java | **Swift** | Go |
|---|---|---|---|---|
| Reusable generic family | ✅ | ✅ | ✅ | rougher |
| One name across arities | ✅ | ❌ | ❌ | ❌ |
| Implicit arm conversion | ✅ | ❌ | ❌ (leading-dot mitigates) | ❌ |
| Generic instance `match<R>` | ✅ | ✅ | ✅ | ❌ |
| Target-typed construction | strong | often | **strong** | limited |
| Exhaustive pattern switch | ❌ (C#14) | ✅ (sealed, 21+) | ✅ **native** | ❌ |
| Primitives without boxing | ✅ | ❌ | ✅ | ✅ |

## API evolution: what breaks when a union's members change

Adding/removing/reordering members changes the type's identity
(`BamlUnion2<Int, MyType>` → `BamlUnion3<Int, String, MyType>`) and renumbers
later arms — identical in C#/Java. Per consumption tier:

| Surface | On member insertion |
|---|---|
| Type-directed construction (`.init(x)`) | **survives** (arm by type — the C# implicit-conversion analog) |
| Type-directed access (`value(as:)`, `holds`, `anyValue` switches) | **survives** |
| Positional (`.t1(x)`, `result.t1`) | renumbers — compile-guided mechanical fix |
| Exhaustive `match` / `switch` | breaks loudly — **by design**: the new arm must be handled (Python's silence here is the bug, not the feature) |
| Spelled-out types | break — mitigate by naming the union in BAML (`type Pay = …` → stable typealias) |

Caveats shared with C# (`FromT0`) and C++ (`get<0>`): duplicate arm
projections make `.init` ambiguous and `value(as:)` first-match — positional
cases remain authoritative. Type-directed construction is convenience, never
case-selection authority; decode is wire-identity-driven regardless.

## Migration

Completed 2026-07-16: rebuilt as Phase 3 (`a3198721f`) with 4a/4b replayed on top; the
named-enum design and its union-specific fixes (literal raw enums, `"r"/"r+"` case-dedup)
are gone. Implementation notes from the rebuild: `_bamlArmIdentity` is a protocol
REQUIREMENT (extension-only members dispatch statically to the nil default); nullable
recursive union aliases (stdlib `json`) keep non-null arms in the nominal enum with `?` at
every reference site; all runtime-library type spellings inside generated scopes must be
fully qualified (`Swift.String`, `Swift.Bool`) or stdlib classes shadow them.
