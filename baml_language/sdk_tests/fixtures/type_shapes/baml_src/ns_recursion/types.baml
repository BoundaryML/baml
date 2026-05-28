// Recursive class scenarios from 18a §"Type shapes / recursive types":
//   - class self-ref:  IntBinaryTree { left/right: IntBinaryTree? } — both
//                      child fields quoted as `"IntBinaryTree"`.
//   - mutual recursion: A.b: B?, B.a: A? — only the *enclosing* class gets
//                      the forward-ref quote.
//   - SCC of 6: {T1,T2,T3} ∩ {T4,T5,T6} where each ref is bare, only
//                      self-ref to the immediate enclosing type is quoted.
//
// The alias self-ref case `type IntLinkedList = { value: int, next: IntLinkedList? }`
// from 18a uses BAML's anonymous-inline-class-in-alias form, which we can't
// reproduce here without confirming the grammar accepts it; the recursive
// alias `RecList` in ns_aliases/ already exercises the codegen path.

class IntBinaryTree {
  value int
  left IntBinaryTree?
  right IntBinaryTree?
}

class A {
  b B?
}

class B {
  a A?
}

// Two SCCs in a single file. {T1,T2,T3} forms one strongly connected
// component; {T4,T5,T6} forms a separate SCC that doesn't reach the
// first.
class T1 {
  via2 T2?
  via3 T3?
}

class T2 {
  via1 T1?
  via3 T3?
}

class T3 {
  via1 T1?
  via2 T2?
}

class T4 {
  via5 T5?
  via6 T6?
}

class T5 {
  via4 T4?
  via6 T6?
}

class T6 {
  via4 T4?
  via5 T5?
}

function round_trip_int_binary_tree(t: IntBinaryTree) -> IntBinaryTree {
  t
}

function round_trip_a(a: A) -> A {
  a
}

function round_trip_b(b: B) -> B {
  b
}

function round_trip_t1(t: T1) -> T1 {
  t
}

function round_trip_t2(t: T2) -> T2 {
  t
}

function round_trip_t3(t: T3) -> T3 {
  t
}

function round_trip_t4(t: T4) -> T4 {
  t
}

function round_trip_t5(t: T5) -> T5 {
  t
}

function round_trip_t6(t: T6) -> T6 {
  t
}
