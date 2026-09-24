// A lambda is named after the body it is lowered inside, and that owner is
// spelled the way the owner's own emitted name ends: `Class.method` for an
// inherent method, `Interface.method` for a default body, and
// `<(target as iface)>.method` for an impl-provided one. Every owner kind
// that is not a free function has a lambda here so the bytecode golden pins
// each spelling; the test executes them all.

interface Runner {
    function run(self, x: int) -> int throws never
    function twice(self, x: int) -> int throws never {
        let go = (v: int) -> int { self.run(v) };
        go(go(x))
    }
}

class Retry {
    base: int

    function scaled(self, x: int) -> int {
        let scale = (v: int) -> int { v * self.base };
        scale(x)
    }

    implements Runner {
        function run(self, x: int) -> int {
            let add = (v: int) -> int { v + self.base };
            add(x)
        }
    }
}

class Other {
    delta: int
}

implement Runner for Other {
    function run(self, x: int) -> int {
        let sub = (v: int) -> int { v - self.delta };
        sub(x)
    }
}

class Boxed<T> {
    inner: T
}

implement<T extends Runner> Runner for Boxed<T> {
    function run(self, x: int) -> int {
        let inner = (v: int) -> int { self.inner.run(v) };
        inner(x)
    }
}

test "lambdas owned by methods of every kind execute" {
    let retry = Retry { base: 10 };
    assert.equal(retry.scaled(4), 40);
    assert.equal(retry.run(4), 14);
    assert.equal(retry.twice(1), 21);
    let other = Other { delta: 3 };
    assert.equal(other.run(10), 7);
    assert.equal(other.twice(10), 4);
    let boxed = Boxed<Other> { inner: other };
    assert.equal(boxed.run(10), 7);
    assert.equal(boxed.twice(10), 4);
}
