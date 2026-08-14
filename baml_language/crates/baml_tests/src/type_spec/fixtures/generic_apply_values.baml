// R4: generic instantiation values (value-position turbofish)
//
// `let f = identity<int>` is rustc's `let f = identity::<i32>`: the
// base callable resolved through the same ladder a call's callee
// takes, its generics instantiated from the WRITTEN args (remaining
// slots fresh), yielding the specialized function VALUE. A method
// base (`p.pick<int>`) binds its receiver like any instance-accessed
// method value.
function giv_id<T>(x: T) -> T throws never {
    x
}

function giv_pair<A, B>(a: A, b: B) -> B throws never {
    b
}

class GivBox {
    v int

    function pick<T>(self, x: T) -> T throws never {
        x
    }
}

function giv_direct() -> int throws never {
    let f = giv_id<int>;
     // ^ (x: int) -> int throws never
    f(5)
}

function giv_multi() -> string throws never {
    let f = giv_pair<int, string>;
     // ^ (a: int, b: string) -> string throws never
    f(1, "hello")
}

function giv_dependent<T>(x: T) -> T throws never {
    let f = giv_id<T>;
     // ^ (x: T) -> T throws never
    f(x)
}

function giv_bound() -> int throws never {
    let p = GivBox { v: 1 };
    let f = p.pick<int>;
     // ^ (x: int) -> int throws never
    f(7)
}
