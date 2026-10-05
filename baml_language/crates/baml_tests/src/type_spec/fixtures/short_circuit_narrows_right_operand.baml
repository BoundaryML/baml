// The right operand of `&&` / `||` is typed with the left operand's facts:
// the true facts for `&&`, the false facts for `||`. The operators
// short-circuit, so the right operand runs only when those facts hold.
// The facts are for the right operand only: after the operator the binding
// has the type from before it. Only a bare local that no closure captures is
// narrowed; a captured local and a field path keep their type.
class A {
    a int
}

class B {
    b string
}

class Holder {
    v A | B
}

function same(v: A | B | null) -> bool throws never {
    true
}

function and_right(x: A | B) -> bool throws never {
    x is A && same(x)
                // ^ user.A
}

function or_right(x: A | B) -> bool throws never {
    x is A || same(x)
                // ^ user.B
}

function not_or_right(x: A | B) -> bool throws never {
    !(x is A) || same(x)
                   // ^ user.A
}

function null_guard(x: A?) -> bool throws never {
    x != null && same(x)
                   // ^ user.A
}

function or_null(x: A?) -> bool throws never {
    x == null || same(x)
                   // ^ user.A
}

function chain(x: A | B | null, y: A | B) -> bool throws never {
    x != null && x is A && y is B && same(x) && same(y)
                                       // ^ user.A
}

function chain_last(x: A | B | null, y: A | B) -> bool throws never {
    x != null && x is A && y is B && same(x) && same(y)
                                                  // ^ user.B
}

function mixed(x: A | B | null) -> bool throws never {
    x != null && (x is B || same(x))
                              // ^ user.A
}

function de_morgan(x: A | B | null) -> bool throws never {
    !(x == null || x is B) && same(x)
                                // ^ user.A
}

function let_value(x: A | B) -> bool throws never {
    let ok = x is A && same(x);
                         // ^ user.A
    ok
}

function after_the_operator(x: A | B) -> bool throws never {
    let ok = x is A && same(x);
    same(x) && ok
      // ^ user.A | user.B
}

function then_branch_keeps_the_facts(x: A | B | null) -> bool throws never {
    if (x != null && x is A) {
        same(x)
          // ^ user.A
    } else {
        false
    }
}

function early_exit(x: A | B | null) -> bool throws never {
    if (x == null || x is B) {
        return false;
    }
    same(x)
      // ^ user.A
}

function while_condition(x: A | B, n: int) -> int throws never {
    let count = 0;
    while (x is A && same(x) && count < n) {
                       // ^ user.A
        count += 1;
    }
    count
}

function assigned_inside(x: A | B) -> bool throws never {
    x is A && {
        x = B { b: "replaced" };
        same(x)
          // ^ user.B
    }
}

function assigned_inside_then_after(x: A | B) -> bool throws never {
    let ok = x is A && {
        x = B { b: "replaced" };
        true
    };
    same(x) && ok
      // ^ user.A | user.B
}

function captured(x: A | B) -> bool throws never {
    let read = () -> bool throws never {
        x is A
    };
    x is A && same(x)
                // ^ user.A | user.B
}

function field_path(h: Holder) -> bool throws never {
    h.v is A && same(h.v)
                  // ^^^ user.A | user.B
}
