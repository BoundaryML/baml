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

// A static on a non-generic class: the method's own bounded `T` is the
// whole frame, inferred from the arguments.
class Pairs {
    function pair_static<T extends Combinable>(a: T, b: T) -> string {
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
    // Two faults: `T = Combinable` is not concrete, and nothing determines
    // the class's `K`.
    Pairer.pair_static(a, b);
    Pairs.pair_static(a, b);
}

function accepted() -> void {
    let a: Combinable = A {};
    pair(A {}, A {});
    pair_with_callback(A {}, A {});
    pair_with_callback(B {}, B {}, done = () -> void { });
    second_bounded(a, A {}, () -> void { });
    unbounded(a);
    Pairer { key: a }.pair(A {}, A {});
    // The type-argument channel fills the class frame and then the
    // method's own: `K` first, then `T`.
    Pairer<B, B>.pair_static(B {}, B {});
    Pairs.pair_static(B {}, B {});
}

// The rule covers an impl header's bounded parameter too: a blanket impl
// applies "to all concrete types satisfying the bounds". `Holder<Combinable>`
// would bind the header's `T` to the existential, and `held` could meet two
// implementors through one `T`. The impl is reached by two roads that must
// agree: a goal with an argument only the impl can supply (`X` below) goes
// through impl selection, which replays the header's bound; a goal with none
// open asks the resolver directly.
interface Holds<X> {
    function held(self) -> X throws never
}

class Holder<T> {
    item T
}

implements<T extends Combinable> Holds<string> for Holder<T> {
    function held(self) -> string throws never {
        self.item.combine(self.item)
    }
}

function holds<X, H extends Holds<X>>(h: H) -> int throws never {
    0
}

function rejected_through_impl_header(h: Holder<Combinable>) -> void {
    holds(h);
    let ground: Holds<string> = h;
}

function accepted_through_impl_header(h: Holder<A>) -> void {
    holds(h);
    let ground: Holds<string> = h;
}

// An alias is its target, so an alias of an existential or of a union is just
// as abstract - a recursive one included - while an alias of a concrete type is
// that type.
type CombinableAlias = Combinable;
type Either = A | B;
type Tree = A | Tree[];
type Concrete = A;

function rejected_through_aliases(c: CombinableAlias, e: Either, t: Tree) -> void {
    pair(c, c);
    pair(e, e);
    pair(t, t);
}

function accepted_through_alias(a: Concrete) -> void {
    pair(a, a);
}

// `unknown` spans every type and `never` none, so neither is concrete either.
function one<T extends Combinable>(a: T) -> string throws never {
    "one"
}

function rejected_top_and_bottom(u: unknown, n: never) -> void {
    one(u);
    one<unknown>(u);
    one(n);
}

// A literal type is a subset of its concrete type, not one: `"a"` uses
// `string`'s implementation of `Combinable`, but it is never a bound's type
// argument, written on a function or on a class alike.
implement Combinable for string {
    function combine(self, other: string) -> string throws never { "s" }
}

enum Side {
    Left,
    Right,
}

implement Combinable for Side {
    function combine(self, other: Side) -> string throws never { "side" }
}

class LiteralHolder<T extends Combinable> {
    value T
}

function literal_type_arguments_rejected() -> void {
    pair<"a">("a", "a");
    pair<Side.Left>(Side.Left, Side.Left);
    let h: LiteralHolder<"a"> = LiteralHolder { value: "a" };
}

function base_type_arguments_accepted() -> void {
    pair<string>("a", "b");
    pair<Side>(Side.Left, Side.Right);
    let h: LiteralHolder<string> = LiteralHolder { value: "a" };
    // A fresh literal VALUE widens to its base when it fills the parameter:
    // only a literal written as the type argument is refused.
    pair("a", "b");
    pair(Side.Left, Side.Right);
    let inferred = LiteralHolder { value: "a" };
    // A type is its value set: a union spelled with a literal beside its base,
    // or with `never`, is the base type and one concrete type.
    pair<"a" | string>("a", "b");
    pair<string | never>("a", "b");
    pair<MaybeA>("a", "b");
}

type MaybeA = "a" | string
