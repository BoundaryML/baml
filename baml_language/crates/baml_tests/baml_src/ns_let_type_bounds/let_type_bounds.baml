// `let type T extends I = t else { … };` checks each bound when the statement
// runs and runs its diverging `else` block when the type misses one. After
// the statement `T` satisfies its bounds statically, including through an
// interface's `requires`: values of type `T` reach the bounds' members, and
// `T` fills a generic parameter with that bound.

interface LtbNamed {
    name: string,
}

interface LtbTagged {
    tag: string,
}

interface LtbGreeter requires LtbNamed {
    function greet(self) -> string throws never {
        "hello, " + self.name
    }
}

class LtbPerson {
    name: string,
    tag: string,

    implements LtbNamed {}

    implements LtbTagged {}

    implements LtbGreeter {}
}

class LtbNamedOnly {
    name: string,

    implements LtbNamed {}
}

class LtbPlain {
    x: int,
}

function ltb_name_of<N extends LtbNamed>(value: N) -> string throws never {
    value.name
}

function ltb_bind_named(t: reflect.Type) -> string {
    let type T extends LtbNamed = t else {
        return "missed " + t.to_string();
    };
    "bound " + reflect.Type.of<T>().to_string()
}

// The name a type value prints with.
function ltb_name(t: reflect.Type) -> string throws never {
    t.to_string()
}

function ltb_bind_both(t: reflect.Type) -> string {
    let type T extends LtbNamed & LtbTagged = t else {
        return "missed";
    };
    let value = baml.json.to<T>({ "name": "Ada", "tag": "admin" });
    value.name + "/" + value.tag
}

test "a type that implements the bound binds" {
    let person = LtbPerson.type();
    assert.equal(ltb_bind_named(person), "bound " + ltb_name(person))
}

test "a type that misses the bound runs the else block" {
    let plain = LtbPlain.type();
    assert.equal(ltb_bind_named(plain), "missed " + ltb_name(plain))
}

test "the else block can skip a loop iteration" {
    let person = LtbPerson.type();
    let named_only = LtbNamedOnly.type();
    let names: string[] = [];
    for (let t in [person, LtbPlain.type(), named_only]) {
        let type T extends LtbNamed = t else {
            continue;
        };
        names.push(baml.json.to<T>({ "name": ltb_name(t), "tag": "t" }).name);
    }
    assert.equal(names, [ltb_name(person), ltb_name(named_only)])
}

test "an intersection bound checks every interface" {
    assert.equal(ltb_bind_both(LtbPerson.type()), "Ada/admin");
    // `LtbNamedOnly` meets the first bound and misses the second.
    assert.equal(ltb_bind_both(LtbNamedOnly.type()), "missed");
    assert.equal(ltb_bind_both(LtbPlain.type()), "missed")
}

test "the bound's members are reachable on values of the bound type" {
    let type T extends LtbGreeter = LtbPerson.type() else {
        throw "LtbPerson implements LtbGreeter";
    };
    let value = baml.json.to<T>({ "name": "Ada", "tag": "admin" });
    assert.equal(value.greet(), "hello, Ada");
    assert.equal(value.name, "Ada")
}

test "a bound is satisfied through requires" {
    // `LtbGreeter requires LtbNamed`, so a `T extends LtbGreeter` is an
    // `LtbNamed` too.
    let type T extends LtbGreeter = LtbPerson.type() else {
        throw "LtbPerson implements LtbGreeter";
    };
    let value = baml.json.to<T>({ "name": "Grace", "tag": "admin" });
    assert.equal(ltb_name_of(value), "Grace")
}

test "a bounded binding fills a bounded generic parameter" {
    let type T extends LtbNamed = LtbNamedOnly.type() else {
        throw "LtbNamedOnly implements LtbNamed";
    };
    let value = baml.json.to<T>({ "name": "Ada" });
    assert.equal(ltb_name_of<T>(value), "Ada");
    assert.equal(ltb_name_of(value), "Ada")
}

// An obligation on `T` can be settled after its block has been checked to
// the closing brace: the element type of `items` is still open when
// `ltb_count_named(items)` is checked, and becomes `T` only through the later
// `push`. The bound still holds there. It is a fact about this parameter
// wherever it appears, since every pass through the statement re-checks it.
function ltb_count_named<N extends LtbNamed>(items: N[]) -> int throws never {
    items.length()
}

function ltb_count_after_late_inference(t: reflect.Type) -> int {
    let type T extends LtbNamed = t else {
        return -1;
    };
    let items = [];
    let before = ltb_count_named(items);
    items.push(baml.json.to<T>({ "name": "Ada" }));
    before + items.length()
}

test "a bound obligation settled after the block closes still holds" {
    assert.equal(ltb_count_after_late_inference(LtbNamedOnly.type()), 1);
    assert.equal(ltb_count_after_late_inference(LtbPlain.type()), -1)
}

interface LtbHolder<T> {
    held: T,
}

class LtbIntBox {
    held: int,

    implements LtbHolder<int> {}
}

class LtbStringBox {
    held: string,

    implements LtbHolder<string> {}
}

function ltb_increment_held(t: reflect.Type) -> string {
    let type B extends LtbHolder<int> = t else {
        return "missed";
    };
    let value = baml.json.to<B>({ "held": 41 });
    (value.held + 1).to_string()
}

test "a generic interface bound checks its type arguments" {
    assert.equal(ltb_increment_held(LtbIntBox.type()), "42");
    assert.equal(ltb_increment_held(LtbStringBox.type()), "missed")
}

// The shape this form exists for: a class built at run time that implements
// a static interface, extracted through a bounded generic function.
interface LtbFooInterface {
    a: int,
}

function ltb_extract<F extends LtbFooInterface>(data: baml.json.json) -> F {
    baml.json.to<F>(data)
}

test "a runtime class extracts through a bounded generic function" {
    let foo_class = reflect.Class.new(
        "Foo",
        { "a": int.type(), "c": int.type() },
        implementations = [reflect.Interface.implementation<LtbFooInterface>().field("a")],
    );
    let type Foo extends LtbFooInterface = foo_class else {
        throw "Foo implements LtbFooInterface";
    };
    let out = ltb_extract<Foo>({ "a": 1, "c": 2 });
    assert.equal(out.a, 1)
}

// A session re-binds an earlier `let type` in each later submission with its
// bounds, so `T` keeps their members there.
test "a session binding keeps its bounds in later submissions" {
    let s = reflect.Session.new();
    s.eval(`interface SessionNamed { name string }`);
    s.eval(`class SessionPerson { name string  implements SessionNamed {} }`);
    s.eval(`let type P extends SessionNamed = SessionPerson.type() else { throw "missed" };`);
    assert.equal(s.eval<string>(`baml.json.to<P>({ "name": "Ada" }).name`), "Ada")
}

test "a session binding checks its bounds where it is written" {
    let s = reflect.Session.new();
    s.eval(`interface SessionNamed { name string }`);
    let missed = s.eval<string>(`let type P extends SessionNamed = int.type() else { throw "missed" }; "bound"`) catch (e) {
        reflect.errors.EvaluationError => e.message,
    };
    assert.equal(missed, "missed")
}

test "a session binding with bounds needs an else block" {
    let s = reflect.Session.new();
    s.eval(`interface SessionNamed { name string }`);
    let code = s.eval<string>(`let type P extends SessionNamed = int.type(); "bound"`) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code,
    };
    assert.equal(code, "E0111")
}

// A bound is a constraint, not a type: it pins only the associated types it
// writes, and the others are free, as for a generic parameter's bound.
interface LtbSource {
    type Item;
    function first(self) -> Self.Item throws never;
}

interface LtbDefaulted {
    type Item = int;
}

class LtbIntSource {
    name: string,
    value: int,

    implements LtbSource {
        type Item = int;

        function first(self) -> int throws never {
            self.value
        }
    }

    implements LtbNamed {}

    implements LtbDefaulted {}
}

class LtbStringSource {
    name: string,
    value: string,

    implements LtbSource {
        type Item = string;

        function first(self) -> string throws never {
            self.value
        }
    }

    implements LtbNamed {}

    implements LtbDefaulted {
        type Item = string;
    }
}

class LtbUnnamedIntSource {
    value: int,

    implements LtbSource {
        type Item = int;

        function first(self) -> int throws never {
            self.value
        }
    }
}

function ltb_any_source(t: reflect.Type) -> string {
    let type S extends LtbSource = t else {
        return "missed";
    };
    "bound"
}

function ltb_int_source(t: reflect.Type) -> string {
    let type S extends LtbSource<Item = int> = t else {
        return "missed";
    };
    let value = baml.json.to<S>({ "name": "n", "value": 41 });
    (value.first() + 1).to_string()
}

function ltb_named_int_source(t: reflect.Type) -> string {
    let type S extends LtbSource<Item = int> & LtbNamed = t else {
        return "missed";
    };
    let value = baml.json.to<S>({ "name": "Ada", "value": 1 });
    value.name + "/" + value.first().to_string()
}

function ltb_any_defaulted(t: reflect.Type) -> string {
    let type D extends LtbDefaulted = t else {
        return "missed";
    };
    "bound"
}

test "a bound leaves the associated types it does not pin free" {
    assert.equal(ltb_any_source(LtbIntSource.type()), "bound");
    assert.equal(ltb_any_source(LtbStringSource.type()), "bound");
    assert.equal(ltb_any_source(LtbPlain.type()), "missed")
}

test "an unpinned associated type is free even where it has a default" {
    assert.equal(ltb_any_defaulted(LtbIntSource.type()), "bound");
    assert.equal(ltb_any_defaulted(LtbStringSource.type()), "bound")
}

test "a bound that pins an associated type misses on another binding" {
    assert.equal(ltb_int_source(LtbIntSource.type()), "42");
    assert.equal(ltb_int_source(LtbStringSource.type()), "missed")
}

test "an intersection checks a pinned and an unpinned bound" {
    assert.equal(ltb_named_int_source(LtbIntSource.type()), "Ada/1");
    // The wrong `Item`: misses the pinned bound.
    assert.equal(ltb_named_int_source(LtbStringSource.type()), "missed");
    // The right `Item` but not named: misses the unpinned bound.
    assert.equal(ltb_named_int_source(LtbUnnamedIntSource.type()), "missed")
}

test "an interface type does not satisfy its own bound" {
    // As for a generic parameter, a bound wants a concrete type.
    assert.equal(ltb_any_source(reflect.Type.of<LtbSource<Item = int>>()), "missed")
}
