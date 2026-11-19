// splitting an alias-typed column into its union members covers it,
// whether the alias is recursive or not, at the top level, in a class
// field, a tuple slot or a list element
type R = int | R[]
type S = int | string
class A {
    a int
}
class B {
    b string
}
type J = A | B | J[]
class Box<T> {
    v T
}

function ra_field(b: Box<R>) -> int {
    let r = match (b) {
     // ^ int
        Box<R> { v: int } => 1,
        Box<R> { v: R[] } => 2
    }
    r
}
function ra_scrut(x: R) -> int {
    let r = match (x) {
     // ^ int
        int => 1,
        R[] => 2
    }
    r
}
class SBox {
    v S
}
function ra_declared_alias_field(b: SBox) -> int {
    let r = match (b) {
     // ^ int
        SBox { v: int } => 1,
        SBox { v: string } => 2
    }
    r
}
function ra_tuple_slot(t: (R, bool)) -> int {
    let r = match (t) {
     // ^ int
        (int, _) => 1,
        (R[], _) => 2
    }
    r
}
function ra_element_split(xs: R[]) -> int {
    let r = match (xs) {
     // ^ int
        [] => 1,
        [int, ..] => 2,
        [R[], ..] => 3
    }
    r
}
function ra_list_split(b: Box<R>) -> int {
    let r = match (b) {
     // ^ int
        Box<R> { v: [] } => 1,
        Box<R> { v: [_, ..] } => 2,
        Box<R> { v: int } => 3
    }
    r
}
function ra_literal_split(b: Box<R>) -> int {
    let r = match (b) {
     // ^ int
        Box<R> { v: 0 } => 1,
        Box<R> { v: int } => 2,
        Box<R> { v: R[] } => 3
    }
    r
}
function ra_class_members(b: Box<J>) -> int {
    let r = match (b) {
     // ^ int
        Box<J> { v: A } => 1,
        Box<J> { v: B } => 2,
        Box<J> { v: J[] } => 3
    }
    r
}
function ra_field_missing_list(b: Box<R>) -> int {
    let r = match (b) { Box<R> { v: int } => 1 };
     // ^ !error
  //        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ non-exhaustive
    r
}
function ra_scrut_missing_int(x: R) -> int {
    let r = match (x) { R[] => 1 };
     // ^ !error
  //        ^^^^^^^^^^^^^^^^^^^^^^ non-exhaustive
    r
}
function ra_class_members_missing(b: Box<J>) -> int {
    let r = match (b) { Box<J> { v: A } => 1, Box<J> { v: J[] } => 3 };
     // ^ !error
  //        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ non-exhaustive
    r
}
function ra_element_missing_list(xs: R[]) -> int {
    let r = match (xs) { [] => 1, [int, ..] => 2 };
     // ^ !error
  //        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ non-exhaustive
    r
}
