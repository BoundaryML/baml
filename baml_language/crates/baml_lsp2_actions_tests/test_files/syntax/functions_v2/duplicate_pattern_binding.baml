class Inner {
  val int
}

class Middle {
  inner Inner
  tag string
}

class Outer {
  middle Middle
  name string
}

enum Color {
  Red
}

class Cat {
  name string
}

class Fish {
  freshwater bool
}

// Binding name collides with destructured field name
function BindAndField() -> int {
  let x: Inner { x } = Inner { val: 1 };
  x
}

// Chain binds same name twice
function ChainBind() -> int {
  let x: let x = 1;
  x
}

// Nested: outer bind "val" collides with inner field bind "val"
function NestedFieldCollision() -> int {
  let val: Inner { val } = Inner { val: 1 };
  val
}

// Deep nesting: bind "tag" at top, destructure hits "tag" three levels in
function DeepNesting() -> string {
  let tag: Outer { middle: Middle { tag, inner: Inner { val } } } = Outer {
    middle: Middle { inner: Inner { val: 1 }, tag: "dup" },
    name: "ok"
  };
  tag
}

// Two fields destructure to same bind name via explicit rebinding
function TwoFieldsSameBind() -> int {
  let Middle { inner: let x, tag: let x } = Middle {
    inner: Inner { val: 1 },
    tag: "oops"
  };
  x
}

// Chain + nested destructure: bind "name" in chain, also destructure "name" from class
function ChainPlusNestedField() -> string {
  let name: Outer { name } = Outer {
    middle: Middle { inner: Inner { val: 0 }, tag: "" },
    name: "collision"
  };
  name
}

// Triple: same name appears in bind, field, AND chain
function TripleBind() -> int {
  let val: Inner { val }: let val = Inner { val: 1 };
  val
}

// Destructure against an unresolved class name
function MissingClassDestructure() -> int {
  let MissingInner { val } = 1;
  val
}

// Destructure a concrete type that exists but is not a class
function EnumDestructure() -> int {
  let Color { x } = 1;
  x
}

// Primitive sugar followed by class destructure in a let pattern
function PrimitiveLetDestructure() -> int {
  let int { val } = 1;
  val
}

// Unresolved class name nested inside a valid class destructure
function NestedMissingClass(x: Outer) -> int {
  let Outer { middle: MissingMiddle { val } } = x;
  val
}

// Non-existent field nested inside a valid class destructure
function NestedMissingField(x: Outer) -> int {
  let Outer { middle: Middle { inner: Inner { missing } } } = x;
  missing
}

// Duplicate field names with explicit aliases should not produce duplicate binding noise
function DuplicateFieldAliases(x: Inner) -> int {
  let Inner { val: let a, val: let b } = x;
  a
}

// Or-pattern with an unresolved class in one alternative
function OrMissingClass(x: Cat) -> string {
  match (x) {
    Cat { name } | MissingPet { name } => name
  }
}

// Or-pattern with the same binding name but a bad field in one alternative
function OrBadField(x: Cat | Fish) -> string {
  match (x) {
    Cat { name } | Fish { name } => name
  }
}

// Chain-internal type mismatch should point at the chain pattern
function ChainTypeMismatch() -> int {
  let x: string: Inner { val } = "nope";
  val
}

//----
//- diagnostics
// Error: identifier `x` is bound more than once in the same pattern
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:29:18 ]
//     │
//  29 │   let x: Inner { x } = Inner { val: 1 };
//     │                  ┬  
//     │                  ╰── identifier `x` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: identifier `x` is bound more than once in the same pattern
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:35:14 ]
//     │
//  35 │   let x: let x = 1;
//     │              ┬  
//     │              ╰── identifier `x` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: identifier `val` is bound more than once in the same pattern
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:41:20 ]
//     │
//  41 │   let val: Inner { val } = Inner { val: 1 };
//     │                    ─┬─  
//     │                     ╰─── identifier `val` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: identifier `tag` is bound more than once in the same pattern
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:47:37 ]
//     │
//  47 │   let tag: Outer { middle: Middle { tag, inner: Inner { val } } } = Outer {
//     │                                     ─┬─  
//     │                                      ╰─── identifier `tag` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: identifier `x` is bound more than once in the same pattern
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:56:39 ]
//     │
//  56 │   let Middle { inner: let x, tag: let x } = Middle {
//     │                                       ┬  
//     │                                       ╰── identifier `x` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: identifier `name` is bound more than once in the same pattern
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:65:21 ]
//     │
//  65 │   let name: Outer { name } = Outer {
//     │                     ──┬─  
//     │                       ╰─── identifier `name` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: identifier `val` is bound more than once in the same pattern
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:74:20 ]
//     │
//  74 │   let val: Inner { val }: let val = Inner { val: 1 };
//     │                    ─┬─  
//     │                     ╰─── identifier `val` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: class `Inner` has no field `x`
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:29:18 ]
//     │
//  29 │   let x: Inner { x } = Inner { val: 1 };
//     │                  ┬  
//     │                  ╰── class `Inner` has no field `x`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: type mismatch: expected int, got string
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:60:3 ]
//     │
//  60 │   x
//     │   ┬  
//     │   ╰── type mismatch: expected int, got string
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: type mismatch: expected int, got user.Inner
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:75:3 ]
//     │
//  75 │   val
//     │   ─┬─  
//     │    ╰─── type mismatch: expected int, got user.Inner
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: unresolved name: MissingInner
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:80:7 ]
//     │
//  80 │   let MissingInner { val } = 1;
//     │       ──────┬─────  
//     │             ╰─────── unresolved name: MissingInner
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: cannot destructure `Color`: only classes can be destructured
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:86:7 ]
//     │
//  86 │   let Color { x } = 1;
//     │       ──┬──  
//     │         ╰──── cannot destructure `Color`: only classes can be destructured
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: cannot destructure `int`: only classes can be destructured
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:92:7 ]
//     │
//  92 │   let int { val } = 1;
//     │       ─┬─  
//     │        ╰─── cannot destructure `int`: only classes can be destructured
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: unresolved name: MissingMiddle
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:98:23 ]
//     │
//  98 │   let Outer { middle: MissingMiddle { val } } = x;
//     │                       ──────┬──────  
//     │                             ╰──────── unresolved name: MissingMiddle
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: class `Inner` has no field `missing`
//      ╭─[ functions_v2_duplicate_pattern_binding.baml:104:47 ]
//      │
//  104 │   let Outer { middle: Middle { inner: Inner { missing } } } = x;
//      │                                               ───┬───  
//      │                                                  ╰───── class `Inner` has no field `missing`
//      │ 
//      │ Note: Error code: E0007
// ─────╯
// Error: field `val` listed more than once in `Inner` destructure
//      ╭─[ functions_v2_duplicate_pattern_binding.baml:110:27 ]
//      │
//  110 │   let Inner { val: let a, val: let b } = x;
//      │                           ─┬─  
//      │                            ╰─── field `val` listed more than once in `Inner` destructure
//      │ 
//      │ Note: Error code: E0007
// ─────╯
// Error: unresolved name: MissingPet
//      ╭─[ functions_v2_duplicate_pattern_binding.baml:117:20 ]
//      │
//  117 │     Cat { name } | MissingPet { name } => name
//      │                    ─────┬────  
//      │                         ╰────── unresolved name: MissingPet
//      │ 
//      │ Note: Error code: E0003
// ─────╯
// Error: class `Fish` has no field `name`
//      ╭─[ functions_v2_duplicate_pattern_binding.baml:124:27 ]
//      │
//  124 │     Cat { name } | Fish { name } => name
//      │                           ──┬─  
//      │                             ╰─── class `Fish` has no field `name`
//      │ 
//      │ Note: Error code: E0007
// ─────╯
// Error: type mismatch: expected string, got user.Inner
//      ╭─[ functions_v2_duplicate_pattern_binding.baml:130:18 ]
//      │
//  130 │   let x: string: Inner { val } = "nope";
//      │                  ──────┬──────  
//      │                        ╰──────── type mismatch: expected string, got user.Inner
//      │ 
//      │ Note: Error code: E0001
// ─────╯
