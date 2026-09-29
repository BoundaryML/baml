// A method's one required `Self`-typed parameter is the value an erased
// `Self` dispatches on, wherever it is declared: `self` is sugar for that
// parameter at position 0, not a separate dispatch road. The VM reads the
// declared position (`VirtualCall.self_arg`), so a `Self` after other
// parameters — or after the method's own type arguments — dispatches exactly
// as a receiver does.
interface Pairable {
    function pair(x: Self, y: int) -> int throws never
    function later(n: int, x: Self) -> int throws never
    function tag<T>(n: int, x: Self, count: T[]) -> int throws never
    function clone(self) -> Self throws never
    function later_twice(self) -> int throws never {
        Pairable.later(2, self) + Pairable.later(2, self)
    }
}

class P {
    v int

    implements Pairable {
        function pair(x: P, y: int) -> int throws never { x.v + y }
        function later(n: int, x: P) -> int throws never { x.v + n }
        function tag<T>(n: int, x: P, count: T[]) -> int throws never { x.v + n + count.length() }
        function clone(self) -> P throws never { P { v: self.v } }
    }
}

class Q {
    w int

    implements Pairable {
        function pair(x: Q, y: int) -> int throws never { x.w * y }
        function later(n: int, x: Q) -> int throws never { x.w * n }
        function tag<T>(n: int, x: Q, count: T[]) -> int throws never { x.w * n + count.length() }
        function clone(self) -> Q throws never { Q { w: self.w } }
    }
}

function via_interface(a: Pairable, y: int) -> int throws never {
    Pairable.pair(a, y)
}

function via_interface_later(n: int, a: Pairable) -> int throws never {
    Pairable.later(n, a)
}

function via_bound<T extends Pairable>(n: int, x: T) -> int throws never {
    Pairable.later(n, x)
}

function via_bound_tagged<T extends Pairable>(x: T) -> int throws never {
    Pairable.tag<string>(1, x, ["a", "b"])
}

test "a self-less method with one leading Self parameter dispatches on an erased Self" {
    let p: Pairable = P { v: 40 };
    let q: Pairable = Q { w: 7 };
    assert.equal(via_interface(p, 2), 42);
    assert.equal(via_interface(q, 6), 42);
    assert.equal(Pairable.pair(P { v: 1 }, 1), 2);
    assert.equal(via_interface(p.clone(), 2), 42);
}

test "a self-less method dispatches on its one Self parameter wherever it is declared" {
    let p: Pairable = P { v: 40 };
    let q: Pairable = Q { w: 7 };
    assert.equal(via_interface_later(2, p), 42);
    assert.equal(via_interface_later(6, q), 42);
    assert.equal(Pairable.later(1, P { v: 1 }), 2);
    assert.equal(via_bound(2, P { v: 40 }), 42);
    assert.equal(via_bound(6, Q { w: 7 }), 42);
}

test "method-level type arguments sit below the dispatched Self argument" {
    let p: Pairable = P { v: 40 };
    let q: Pairable = Q { w: 8 };
    assert.equal(Pairable.tag(1, p, ["a"]), 42);
    assert.equal(Pairable.tag<int>(5, q, [1, 2]), 42);
    assert.equal(via_bound_tagged(P { v: 39 }), 42);
    assert.equal(via_bound_tagged(Q { w: 40 }), 42);
}

test "a default body dispatches Self from a later argument" {
    let p: Pairable = P { v: 20 };
    let q: Pairable = Q { w: 11 };
    assert.equal(p.later_twice(), 44);
    assert.equal(q.later_twice(), 44);
}

test "a runtime id rides a virtual call dispatched on a later argument" {
    let p: Pairable = P { v: 40 };
    let q: Pairable = Q { w: 7 };
    assert.equal(Pairable.later(2, p, $id = boundary.id()), 42);
    assert.equal(Pairable.later(6, q, $id = boundary.id()), 42);
}

// The served lane: an interface row of a runtime-compiled package carries
// its parameter modes, so the consumer's checker admits the same dispatch
// and the VM reads the same declared position through the mount.
test "a served interface method dispatches on its later Self parameter" {
    let lib = reflect.Package.compile(
        {
            "lib.baml": `
                interface Pairable {
                    function later(n: int, x: Self) -> int throws never
                }

                class P {
                    v int

                    implements Pairable {
                        function later(n: int, x: P) -> int throws never { x.v + n }
                    }
                }

                class Q {
                    w int

                    implements Pairable {
                        function later(n: int, x: Q) -> int throws never { x.w * n }
                    }
                }

                function make_p(v: int) -> Pairable throws never { P { v: v } }
                function make_q(w: int) -> Pairable throws never { Q { w: w } }
            `,
        },
    );
    let consumer = reflect.Package.compile(
        {
            "main.baml": `
                function run(n: int, from_q: bool) -> int throws never {
                    let a: app.Pairable = if (from_q) { app.make_q(7) } else { app.make_p(40) };
                    app.Pairable.later(n, a)
                }

                function bounded<T extends app.Pairable>(n: int, x: T) -> int throws never {
                    app.Pairable.later(n, x)
                }

                function run_bounded(n: int) -> int throws never {
                    bounded(n, app.P { v: 40 })
                }
            `,
        },
        packages = { "app": lib },
    );
    let run = consumer.get_function<(int, bool) -> int>("root.run") ?? throw "missing root.run";
    assert.equal(run(2, false), 42);
    assert.equal(run(6, true), 42);
    let run_bounded = consumer.get_function<(int) -> int>("root.run_bounded") ?? throw "missing root.run_bounded";
    assert.equal(run_bounded(2), 42);
}
