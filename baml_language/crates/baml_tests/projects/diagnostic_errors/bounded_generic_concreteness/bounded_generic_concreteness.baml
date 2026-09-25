// A type parameter with an interface bound takes only a CONCRETE type: an
// existential (or a union) has no single implementor to dispatch on, so
// `a.combine(b)` inside `pair` could meet two different implementors. The
// rule judges the callee's own parameters wherever they sit in its frame — a
// callback parameter, whose omitted `throws` adds an effect parameter after
// them, does not move them out of its reach — and leaves the owner's prefix
// to the receiver.
interface Combinable {
    function combine(self, other: Self) -> string throws never
}

class A {
    implements Combinable {
        function combine(self, other: A) -> string throws never { "a" }
    }
}

class B {
    implements Combinable {
        function combine(self, other: B) -> string throws never { "b" }
    }
}

function pair<T extends Combinable>(a: T, b: T) -> string throws never {
    a.combine(b)
}

function pair_with_callback<T extends Combinable>(
    a: T,
    b: T,
    done: (() -> void)? = null,
) -> string {
    a.combine(b)
}

function second_bounded<U, T extends Combinable>(u: U, a: T, done: () -> void) -> U {
    done();
    u
}

function unbounded<T>(a: T, done: (() -> void)? = null) -> T {
    a
}

class Pairer<K> {
    key K

    function pair<T extends Combinable>(self, a: T, b: T, done: (() -> void)? = null) -> string {
        a.combine(b)
    }

    function pair_static<T extends Combinable>(a: T, b: T, done: (() -> void)? = null) -> string {
        a.combine(b)
    }
}

function rejected() -> void {
    let a: Combinable = A {};
    let b: Combinable = B {};
    let either: A | B = A {};
    pair(a, b);
    pair_with_callback(a, b);
    pair_with_callback(a, b, done = () -> void { });
    pair_with_callback<Combinable>(a, b);
    pair_with_callback(either, either);
    second_bounded(1, a, () -> void { });
    Pairer { key: a }.pair(a, b);
    Pairer.pair_static(a, b);
}

function accepted() -> void {
    let a: Combinable = A {};
    pair(A {}, A {});
    pair_with_callback(A {}, A {});
    pair_with_callback(B {}, B {}, done = () -> void { });
    second_bounded(a, A {}, () -> void { });
    unbounded(a);
    Pairer { key: a }.pair(A {}, A {});
    Pairer.pair_static(B {}, B {});
}
