// A method whose one `Self`-typed parameter comes first dispatches on an
// erased `Self` exactly as a `self` method does: `self` is sugar for that
// parameter, not a separate dispatch road.
interface Pairable {
    function pair(x: Self, y: int) -> int throws never
    function clone(self) -> Self throws never
}

class P {
    v int

    implements Pairable {
        function pair(x: P, y: int) -> int throws never { x.v + y }
        function clone(self) -> P throws never { P { v: self.v } }
    }
}

class Q {
    w int

    implements Pairable {
        function pair(x: Q, y: int) -> int throws never { x.w * y }
        function clone(self) -> Q throws never { Q { w: self.w } }
    }
}

function via_interface(a: Pairable, y: int) -> int throws never {
    Pairable.pair(a, y)
}

test "a self-less method with one leading Self parameter dispatches on an erased Self" {
    let p: Pairable = P { v: 40 };
    let q: Pairable = Q { w: 7 };
    assert.equal(via_interface(p, 2), 42);
    assert.equal(via_interface(q, 6), 42);
    assert.equal(Pairable.pair(P { v: 1 }, 1), 2);
    assert.equal(via_interface(p.clone(), 2), 42);
}
