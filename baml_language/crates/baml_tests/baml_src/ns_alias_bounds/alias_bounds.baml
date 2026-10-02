// A type alias is its target in every context (TYPE_SYSTEM.md, "Type Aliases
// and Recursive Types"), so a value typed by an alias satisfies an interface
// bound exactly when the aliased type does - through a blanket impl, an impl
// whose header needs structure below the alias, or an interface whose
// arguments are still being inferred.

interface Show {
    function show(self) -> string throws never
}

interface Conv<A> {
    function to(self) -> A throws never
}

interface Same {
    function same(self, other: Self) -> bool throws never
}

class Box<T> {
    v: T,
}

implements<T> Show for Box<T> {
    function show(self) -> string throws never { "box" }
}

implements<T> Conv<T> for Box<T> {
    function to(self) -> T throws never { self.v }
}

implements<T> Same for Box<T> {
    function same(self, other: Box<T>) -> bool throws never { true }
}

class Wrapper<T> {
    w: T,
}

implements<T extends Show> Show for Wrapper<T> {
    function show(self) -> string throws never { `wrapper(${self.w.show()})` }
}

implements<T extends Show> Show for T[] {
    function show(self) -> string throws never { `list of ${self.length()}` }
}

interface Loud {
    function loud(self) -> string throws never
}

implements<T extends Show> Loud for T {
    function loud(self) -> string throws never { `${self.show()}!` }
}

// Headers that need structure BELOW an alias: `Wrapper<IntBox>` meets
// `Wrapper<Box<T>>` only once the alias unfolds.
interface Deep {
    function deep(self) -> string throws never
}

implements<T> Deep for Wrapper<Box<T>> {
    function deep(self) -> string throws never { "deep" }
}

interface DeepOf<A> {
    function deep_of(self) -> A throws never
}

implements<T> DeepOf<T> for Wrapper<Box<T>> {
    function deep_of(self) -> T throws never { self.w.v }
}

class Counter {
    n: int,
    implements Conv<int> {
        function to(self) -> int throws never { self.n }
    }
}

type IntBox = Box<int>;
type IntBox2 = IntBox;
type ConvInt = Conv<int>;
type Counts = Box<int> | Counter;
type Ints = int[];
type Deeply = Wrapper<IntBox>;

function show_it<T extends Show>(t: T) -> string throws never { t.show() }
function conv_it<A, T extends Conv<A>>(t: T) -> A throws never { t.to() }
function takes_conv<A>(c: Conv<A>) -> A throws never { c.to() }
function same_it<T extends Same>(a: T, b: T) -> bool throws never { a.same(b) }
function deep_it<T extends Deep>(t: T) -> string throws never { t.deep() }
function deep_of_it<A, T extends DeepOf<A>>(t: T) -> A throws never { t.deep_of() }

function int_box() -> IntBox { Box { v: 7 } }

class Holder {
    b: IntBox,
}

test "an alias satisfies a bound its target does" {
    let b: IntBox = Box { v: 7 };
    assert.equal(show_it(b), "box");
    assert.equal(show_it<IntBox>(b), "box")
}

test "an alias of an alias satisfies the bound" {
    let b: IntBox2 = Box { v: 7 };
    assert.equal(show_it(b), "box")
}

test "an alias satisfies a bound whose interface argument is inferred" {
    let b: IntBox = Box { v: 7 };
    assert.equal(conv_it(b), 7)
}

test "an alias coerces to an existential whose argument is inferred" {
    let b: IntBox = Box { v: 7 };
    assert.equal(takes_conv(b), 7)
}

test "an alias of an existential coerces to it" {
    let c: ConvInt = Box { v: 7 };
    assert.equal(takes_conv(c), 7)
}

test "a union alias coerces to an existential every member implements" {
    let boxed: Counts = Box { v: 7 };
    let counted: Counts = Counter { n: 8 };
    assert.equal([takes_conv(boxed), takes_conv(counted)], [7, 8])
}

test "an alias meets a bound on the impl header" {
    let w: Wrapper<IntBox> = Wrapper { w: Box { v: 7 } };
    assert.equal(show_it(w), "wrapper(box)")
}

test "an alias meets the bound of a blanket impl for its list" {
    let xs: IntBox[] = [Box { v: 1 }, Box { v: 2 }];
    assert.equal(show_it(xs), "list of 2")
}

test "an alias meets a Self-typed bound" {
    let b: IntBox = Box { v: 7 };
    let c: Box<int> = Box { v: 8 };
    assert.is_true(same_it(b, c))
}

test "an alias nested under a header's structure unfolds to match it" {
    let w: Wrapper<IntBox> = Wrapper { w: Box { v: 7 } };
    assert.equal(deep_it(w), "deep");
    assert.equal(deep_of_it(w), 7)
}

test "an interface member resolves on an aliased receiver" {
    let b: IntBox = Box { v: 7 };
    let w: Wrapper<IntBox> = Wrapper { w: Box { v: 7 } };
    let d: Deeply = Wrapper { w: Box { v: 7 } };
    assert.equal([b.show(), b.loud(), w.deep(), d.deep()], ["box", "box!", "deep", "deep"])
}

test "an alias satisfies a bound wherever its value flows" {
    let h = Holder { b: Box { v: 7 } };
    let f = (b: IntBox) -> string { show_it(b) };
    let flows = [show_it(h.b), show_it(int_box()), f(Box { v: 1 })];
    assert.equal(flows, ["box", "box", "box"])
}

test "a stdlib bound accepts an aliased container" {
    let xs: Ints[] = [[1, 2], [3]];
    let total = 0;
    for (let x in baml.iter.flatten(xs.iter())) {
        total += x;
    }
    assert.equal(total, 6)
}
