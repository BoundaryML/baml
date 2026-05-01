class Inner {
  val int
}

class A {
  type "a"
  val int
}

class B {
  type "b"
  val int
}

class C {
  type "c"
  val int
}

type AB = A | B
type ABC = A | B | C

// Top-level split with mismatched binding names across alternatives
function MismatchedNames(x: AB) -> int {
  match (x) {
    let x: A { val } | let y: B { val } => val
  }
}

// Mixed chain-local and top-level in same pipe chain:
// Parses as: let x: (A { val } | PATTERN(B { val } | PATTERN(let x: C { val })))
// The inner PATTERN creates Or([B{val}, Bind(x)->C{val}]) — binding mismatch
function MixedChainLocalAndTopLevel(x: ABC) -> int {
  match (x) {
    let x: A { val } | B { val } | let x: C { val } => val
  }
}

// Duplicate binding inside one side of a top-level split
function DuplicateInTopLevelAlt(x: AB) -> int {
  match (x) {
    let x: A { x } | let x: B { x } => x
  }
}

//----
//- diagnostics
// Error: or-pattern alternatives must bind the same variables; not bound in all alternatives: `x`; only bound in some alternatives: `y`
//     ╭─[ match_match_or_pattern_errors.baml:26:28 ]
//     │
//  26 │     let x: A { val } | let y: B { val } => val
//     │                            ┬  
//     │                            ╰── or-pattern alternatives must bind the same variables; not bound in all alternatives: `x`; only bound in some alternatives: `y`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: identifier `x` is bound more than once in the same pattern
//     ╭─[ match_match_or_pattern_errors.baml:42:16 ]
//     │
//  42 │     let x: A { x } | let x: B { x } => x
//     │                ┬  
//     │                ╰── identifier `x` is bound more than once in the same pattern
//     │ 
//     │ Note: Error code: E0094
// ────╯
// Error: or-pattern binding `x` has type `user.A | user.B` in one alternative but `user.C` in another
//     ╭─[ match_match_or_pattern_errors.baml:35:40 ]
//     │
//  35 │     let x: A { val } | B { val } | let x: C { val } => val
//     │                                        ┬  
//     │                                        ╰── or-pattern binding `x` has type `user.A | user.B` in one alternative but `user.C` in another
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: class `A` has no field `x`
//     ╭─[ match_match_or_pattern_errors.baml:42:16 ]
//     │
//  42 │     let x: A { x } | let x: B { x } => x
//     │                ┬  
//     │                ╰── class `A` has no field `x`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: class `B` has no field `x`
//     ╭─[ match_match_or_pattern_errors.baml:42:33 ]
//     │
//  42 │     let x: A { x } | let x: B { x } => x
//     │                                 ┬  
//     │                                 ╰── class `B` has no field `x`
//     │ 
//     │ Note: Error code: E0007
// ────╯
