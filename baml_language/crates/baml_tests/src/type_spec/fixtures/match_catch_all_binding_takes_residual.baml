// A bare binding in a catch-all arm binds what the earlier arms left, as the
// scrutinee itself reads there (B-774): after `null => ..` it is not null.
// The binding needs no narrowable scrutinee, so it also holds for a field
// path, and for every kind of type (a function type has no other spelling
// that could be written as the arm's pattern cheaply). A guarded arm
// subtracts nothing, because its guard can send the value on.
class A {
    a int
}

class B {
    b string
}

class Holder {
    hook ((int) -> string throws never)?
    item (A | B)?

    function call(self, n: int) -> string throws never {
        match (self.hook) {
            null => "none",
            let hook => hook(n),
             // ^^^^ (int) -> string throws never
        }
    }
}

function parameter(item: A | B | null) -> string throws never {
    match (item) {
        null => "none",
        let a: A => "a",
        let rest => rest.b,
         // ^^^^ user.B
    }
}

function local(n: int) -> int throws never {
    let item: A? = if (n > 0) { A { a: n } } else { null };
    match (item) {
        null => 0,
        let found => found.a,
         // ^^^^^ user.A
    }
}

function field_path(holder: Holder) -> string throws never {
    match (holder.item) {
        null => "none",
        let a: A => "a",
        let rest => rest.b,
         // ^^^^ user.B
    }
}

function function_parameter(hook: ((int) -> string throws never)?, n: int) -> string throws never {
    match (hook) {
        null => "none",
        let known => known(n),
         // ^^^^^ (int) -> string throws never
    }
}

function guarded_arm_subtracts_nothing(item: A | B) -> int throws never {
    match (item) {
        let a: A if a.a > 0 => 1,
        let rest => match (rest) {
         // ^^^^ user.A | user.B
            let a: A => 2,
            let b: B => 3,
        },
    }
}

// An alternative of an or-pattern is tried on the value that reached the
// arm, so a bare alternative binds the residual too, and agrees with a typed
// alternative that names it.
function or_alternatives(item: A?) -> int throws never {
    match (item) {
        null => 0,
        let first | let first => first.a,
         // ^^^^^ user.A
    }
}

function typed_and_bare_alternatives(item: A | B | null) -> string throws never {
    match (item) {
        null => "none",
        let a: A => "a",
        (let rest: B) | (let rest) => rest.b,
          // ^^^^ user.B
    }
}

function chained_binding(item: A?) -> int throws never {
    match (item) {
        null => 0,
        let outer: let inner => outer.a + inner.a,
         // ^^^^^ user.A
    }
}
