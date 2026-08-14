// Instance-accessed methods are receiver-BOUND values
//
// `c.bump` as a VALUE consumes the `self` slot (the runtime hands out
// a bound closure), so its type is the signature minus `self`. The
// static/UFCS spelling (`BmvCounter.bump`) keeps the full signature -
// there the receiver arrives as the written first argument.
class BmvCounter {
    n int

    function bump(self, d: int) -> int throws never {
        self.n + d
    }
}

function grab(c: BmvCounter) -> int throws never {
    let f = c.bump;
     // ^ (d: int) -> int throws never
    f(2)
}

function ufcs(c: BmvCounter) -> int throws never {
    let g = BmvCounter.bump;
     // ^ (self: user.BmvCounter, d: int) -> int throws never
    g(c, 3)
}
