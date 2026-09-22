// Object safety through an erased `Self` (an interface-existential): a
// method dispatches on an erased `Self` iff exactly one of its parameters is
// typed `Self` and that parameter comes first — the `self` receiver being just
// such a parameter — and `Self` occurs nowhere else (a bare `-> Self` is
// fine). `self` is only the sugar that lets `Pairable.pair(a, 1)` be written
// `a.pair(1)`; a method without it is a static, which an instance does not
// have. A union is no relaxation in argument position: `Self?` and
// `Self | int` each name the implementor, so they are second `Self`
// parameters, not covariant widenings.
interface Pairable {
    function pair(x: Self, y: int) -> int throws never
    function both(x: Self, y: Self) -> int throws never
    function later(n: int, x: Self) -> int throws never
    function make(n: int) -> Self throws never
    function twice(self, other: Self) -> int throws never
    function maybe(self, other: Self?) -> int throws never
    function eithered(self, other: Self | int) -> int throws never
    function boxed(self) -> Self[] throws never
    function clone(self) -> Self throws never
}

class P {
    v int

    implements Pairable {
        function pair(x: P, y: int) -> int throws never { x.v + y }
        function both(x: P, y: P) -> int throws never { x.v + y.v }
        function later(n: int, x: P) -> int throws never { x.v + n }
        function make(n: int) -> P throws never { P { v: n } }
        function twice(self, other: P) -> int throws never { self.v + other.v }
        function maybe(self, other: P?) -> int throws never {
            match (other) {
                let p: P => self.v + p.v,
                null => self.v,
            }
        }
        function eithered(self, other: P | int) -> int throws never {
            match (other) {
                let p: P => self.v + p.v,
                let n: int => self.v + n,
            }
        }
        function boxed(self) -> P[] throws never { [self] }
        function clone(self) -> P throws never { P { v: self.v } }
    }
}

function through_existential(a: Pairable, b: Pairable) -> int throws never {
    // legal: one `Self`, first — dispatched from `a`
    let dispatched = Pairable.pair(a, 1);
    // error: a static is not an instance member
    let on_instance = a.pair(1);
    // error: two `Self` parameters
    let two_selfs = Pairable.both(a, b);
    // error: the `Self` parameter is not first — nothing to dispatch from
    let not_first = Pairable.later(1, a);
    // error: no `Self` parameter at all
    let no_self = Pairable.make(1);
    // error: `self` plus another `Self`
    let receiver_and_self = a.twice(b);
    // error: `Self?` is a second `Self` parameter — `b` could be any other
    // implementor, and the callee is compiled for one
    let optional_self = a.maybe(b);
    // error: `Self | int` likewise
    let union_self = a.eithered(b);
    // error: `Self[]`
    let nested = a.boxed();
    // legal: bare `-> Self`
    let legal = a.clone();
    dispatched
}

function concrete(a: P, b: P) -> int throws never {
    P.pair(a, 1) + P.both(a, b) + P.later(1, a) + P.make(1).v + a.twice(b) + a.boxed().length()
        + a.clone().v + a.maybe(b) + a.eithered(b)
}
