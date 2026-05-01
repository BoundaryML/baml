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

class Dog {
  type "dog"
  name string
}

class Cat {
  type "cat"
  name string
}

type Pet = Cat | Dog

// Arm bind collides with destructure field
function ArmBindVsField() -> string {
  match (Inner { val: 1 }) {
    let val: Inner { val } => "bad"
  }
}

// Deep destructure in match arm: bind "tag" at arm root, destructure hits "tag" nested
function DeepArmDestructure() -> string {
  match (Outer { middle: Middle { inner: Inner { val: 0 }, tag: "deep" }, name: "hi" }) {
    let tag: Outer { middle: Middle { tag } } => tag
  }
}

// Two fields rebound to same name inside match arm
function TwoFieldsSameBindInArm() -> string {
  match (Middle { inner: Inner { val: 1 }, tag: "x" }) {
    let Middle { inner: let x, tag: let x } => "bad"
  }
}

// Triple: bind + field + chain in a match arm
function TripleInMatchArm() -> int {
  match (Inner { val: 42 }) {
    let val: Inner { val }: let val => val
  }
}

// Multiple arms, only some have duplicates
function MixedArms() -> string {
  match (Inner { val: 1 }) {
    let x: Inner { val } => "ok",
    let val: Inner { val } => "bad"
  }
}

// Or-pattern where each alternative has the duplicate independently
function OrPatternDuplicates(x: int) -> int {
  match (x) {
    let x: let x | let x: let x => x
  }
}

// Match on union with deep destructure collision per arm
function UnionDeepCollision(pet: Pet) -> string {
  match (pet) {
    let name: Cat { name } => name,
    let name: Dog { name } => name
  }
}

//----
//- diagnostics
// Error: identifier `val` is bound more than once in the same pattern
//     ╭─[ match_match_duplicate_bindings.baml:30:22 ]
//     │
//  30 │     let val: Inner { val } => "bad"
//     │                      ─┬─  
//     │                       ╰─── identifier `val` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: identifier `tag` is bound more than once in the same pattern
//     ╭─[ match_match_duplicate_bindings.baml:37:39 ]
//     │
//  37 │     let tag: Outer { middle: Middle { tag } } => tag
//     │                                       ─┬─  
//     │                                        ╰─── identifier `tag` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: identifier `x` is bound more than once in the same pattern
//     ╭─[ match_match_duplicate_bindings.baml:44:41 ]
//     │
//  44 │     let Middle { inner: let x, tag: let x } => "bad"
//     │                                         ┬  
//     │                                         ╰── identifier `x` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: identifier `val` is bound more than once in the same pattern
//     ╭─[ match_match_duplicate_bindings.baml:51:22 ]
//     │
//  51 │     let val: Inner { val }: let val => val
//     │                      ─┬─  
//     │                       ╰─── identifier `val` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: identifier `val` is bound more than once in the same pattern
//     ╭─[ match_match_duplicate_bindings.baml:59:22 ]
//     │
//  59 │     let val: Inner { val } => "bad"
//     │                      ─┬─  
//     │                       ╰─── identifier `val` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: identifier `x` is bound more than once in the same pattern
//     ╭─[ match_match_duplicate_bindings.baml:66:16 ]
//     │
//  66 │     let x: let x | let x: let x => x
//     │                ┬  
//     │                ╰── identifier `x` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: identifier `name` is bound more than once in the same pattern
//     ╭─[ match_match_duplicate_bindings.baml:73:21 ]
//     │
//  73 │     let name: Cat { name } => name,
//     │                     ──┬─  
//     │                       ╰─── identifier `name` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: identifier `name` is bound more than once in the same pattern
//     ╭─[ match_match_duplicate_bindings.baml:74:21 ]
//     │
//  74 │     let name: Dog { name } => name
//     │                     ──┬─  
//     │                       ╰─── identifier `name` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: type mismatch: expected int, got user.Inner
//     ╭─[ match_match_duplicate_bindings.baml:49:37 ]
//     │
//  49 │ ╭─▶ function TripleInMatchArm() -> int {
//     ┆ ┆   
//  52 │ ├─▶   }
//     │ │         
//     │ ╰───────── type mismatch: expected int, got user.Inner
//     │     
//     │     Note: Error code: E0001
// ────╯
