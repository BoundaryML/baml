// An operand that may be skipped - the right operand of `||`/`&&`, a `??`
// fallback, the links of a `?.` chain after the first `?.` - or a catch arm
// may not run, so its divergence is not the enclosing expression's: the flow
// continues past it, and a write after it is seen by the merge at the end of
// the loop or branch. A write INSIDE a skipped operand holds on one path only,
// so after it the binding keeps a refinement only where both paths agree.

class Box {
    function m(self, v: int) -> int throws never {
        v
    }
}

function thrower(a: int) -> int throws string {
    if (a > 0) {
        throw "boom";
    }
    a
}

function or_right_operand_divergence_is_scoped(a: int, s: string, xs: int[]) -> int | string {
    let x: int | string = a;
    x = a;
    for let v in xs {
        let ok = v < 5 || { return 0; };
        x = s;
    }
    x
//  ^ int | string
}

function null_coalesce_fallback_divergence_is_scoped(a: int, s: string, o: int?, xs: int[]) -> int | string {
    let x: int | string = a;
    x = a;
    for let v in xs {
        let w = o ?? { return 0; };
        x = s;
    }
    x
//  ^ int | string
}

function catch_arm_divergence_is_scoped(a: int, s: string, xs: int[]) -> int | string {
    let x: int | string = a;
    x = a;
    for let v in xs {
        let r = thrower(v) catch (e) { _ => { return 0; } };
        x = s;
    }
    x
//  ^ int | string
}

function if_branch_divergence_is_scoped(a: int, s: string, o: int?) -> int | string {
    let x: int | string = a;
    x = a;
    if (a > 0) {
        x = s;
        let w = o ?? { return 0; };
    }
    x
//  ^ int | string
}

function and_right_operand_divergence_is_scoped(a: int, s: string) -> int | string {
    let x: int | string = a;
    x = a;
    if (a > 0) {
        let ok = a < 5 && { return 0; };
        x = s;
    }
    x
//  ^ int | string
}

function optional_call_argument_divergence_is_scoped(a: int, s: string, b: Box?) -> int | string {
    let x: int | string = a;
    x = a;
    if (a > 0) {
        let r = b?.m({ return 0; });
        x = s;
    }
    x
//  ^ int | string
}

// A write inside a skipped operand is a maybe-write: the paths join.
function null_coalesce_fallback_write_joins(a: int, s: string, o: int?) -> int | string {
    let x: int | string = a;
    x = a;
    let w = o ?? { x = s; 0 };
    x
//  ^ int | string
}

function optional_call_argument_write_joins(a: int, s: string, b: Box?) -> int | string {
    let x: int | string = a;
    x = a;
    let r = b?.m({ x = s; 1 });
    x
//  ^ int | string
}

function optional_index_write_joins(a: int, s: string, xs: int[]?) -> int | string {
    let x: int | string = a;
    x = a;
    let r = xs?.[{ x = s; 0 }];
    x
//  ^ int | string
}

function optional_call_of_a_value_write_joins(a: int, s: string, f: ((v: int) -> int)?) -> int | string {
    let x: int | string = a;
    x = a;
    let r = f?.({ x = s; 1 });
    x
//  ^ int | string
}

// The base of the first `?.` link always runs: its write is not a maybe-write.
function chain_prefix_write_holds(a: int, s: string, b: Box?) -> string {
    let x: int | string = a;
    x = a;
    let r = ({ x = s; b })?.m(1);
    x
//  ^ string
}

// A skipped operand that diverged is not a path that reaches the join.
function diverging_fallback_write_does_not_join(a: int, s: string, o: int?) -> int {
    let x: int | string = a;
    x = a;
    let w = o ?? { x = s; return 0; };
    x
//  ^ int
}
