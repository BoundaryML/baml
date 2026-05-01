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

//----
//- diagnostics
// Error: class `Inner` has no field `x`
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:17:24 ]
//     │
//  17 │   let x: Inner { x } = Inner { val: 1 };
//     │                        ────────┬───────  
//     │                                ╰───────── class `Inner` has no field `x`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: identifier `x` is bound more than once in the same pattern
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:17:24 ]
//     │
//  17 │   let x: Inner { x } = Inner { val: 1 };
//     │                        ────────┬───────  
//     │                                ╰───────── identifier `x` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: identifier `x` is bound more than once in the same pattern
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:23:18 ]
//     │
//  23 │   let x: let x = 1;
//     │                  ┬  
//     │                  ╰── identifier `x` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: identifier `val` is bound more than once in the same pattern
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:29:28 ]
//     │
//  29 │   let val: Inner { val } = Inner { val: 1 };
//     │                            ────────┬───────  
//     │                                    ╰───────── identifier `val` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: identifier `tag` is bound more than once in the same pattern
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:35:69 ]
//     │
//  35 │ ╭─▶   let tag: Outer { middle: Middle { tag, inner: Inner { val } } } = Outer {
//     ┆ ┆   
//  38 │ ├─▶   };
//     │ │          
//     │ ╰────────── identifier `tag` is bound more than once in the same pattern
//     │     
//     │     Note: Error code: E0001
// ────╯
// Error: identifier `x` is bound more than once in the same pattern
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:44:45 ]
//     │
//  44 │ ╭─▶   let Middle { inner: let x, tag: let x } = Middle {
//     ┆ ┆   
//  47 │ ├─▶   };
//     │ │          
//     │ ╰────────── identifier `x` is bound more than once in the same pattern
//     │     
//     │     Note: Error code: E0001
// ────╯
// Error: type mismatch: expected int, got string
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:48:3 ]
//     │
//  48 │   x
//     │   ┬  
//     │   ╰── type mismatch: expected int, got string
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: identifier `name` is bound more than once in the same pattern
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:53:30 ]
//     │
//  53 │ ╭─▶   let name: Outer { name } = Outer {
//     ┆ ┆   
//  56 │ ├─▶   };
//     │ │          
//     │ ╰────────── identifier `name` is bound more than once in the same pattern
//     │     
//     │     Note: Error code: E0001
// ────╯
// Error: identifier `val` is bound more than once in the same pattern
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:62:37 ]
//     │
//  62 │   let val: Inner { val }: let val = Inner { val: 1 };
//     │                                     ────────┬───────  
//     │                                             ╰───────── identifier `val` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: type mismatch: expected int, got user.Inner
//     ╭─[ functions_v2_duplicate_pattern_binding.baml:63:3 ]
//     │
//  63 │   val
//     │   ─┬─  
//     │    ╰─── type mismatch: expected int, got user.Inner
//     │ 
//     │ Note: Error code: E0001
// ────╯
