// `let type T extends I = t;` checks each bound when the statement runs and
// throws `reflect.errors.TypeMismatch` naming the first interface the type
// misses, or runs a diverging `else` block instead. After the statement `T`
// satisfies its bounds statically, including through an interface's
// `requires`: values of type `T` reach the bounds' members, and `T` fills a
// generic parameter with that bound.

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
    let type T extends LtbNamed = t;
    "bound " + reflect.Type.of<T>().to_string()
}

function ltb_bind_named_or_else(t: reflect.Type) -> string {
    let type T extends LtbNamed = t else {
        return "missed " + t.to_string();
    };
    "bound " + reflect.Type.of<T>().to_string()
}

// The name a type value prints with, as the bound check's message spells it.
function ltb_name(t: reflect.Type) -> string throws never {
    t.to_string()
}

function ltb_bind_both(t: reflect.Type) -> string {
    let type T extends LtbNamed & LtbTagged = t;
    let value = baml.json.to<T>({ "name": "Ada", "tag": "admin" });
    value.name + "/" + value.tag
}

test "a type that implements the bound binds" {
    let person = reflect.Type.of<LtbPerson>();
    assert.equal(ltb_bind_named(person), "bound " + ltb_name(person))
}

test "a type that misses the bound throws TypeMismatch naming the interface" {
    let plain = reflect.Type.of<LtbPlain>();
    let result = ltb_bind_named(plain) catch (e) {
        reflect.errors.TypeMismatch => "threw: " + e.message,
    };
    assert.equal(
        result,
        "threw: type "
            + ltb_name(plain)
            + " does not implement interface "
            + ltb_name(reflect.Type.of<LtbNamed>()),
    )
}

test "the else block runs instead of the throw" {
    let plain = reflect.Type.of<LtbPlain>();
    let person = reflect.Type.of<LtbPerson>();
    assert.equal(ltb_bind_named_or_else(plain), "missed " + ltb_name(plain));
    assert.equal(ltb_bind_named_or_else(person), "bound " + ltb_name(person))
}

test "the else block can skip a loop iteration" {
    let person = reflect.Type.of<LtbPerson>();
    let named_only = reflect.Type.of<LtbNamedOnly>();
    let names: string[] = [];
    for (let t in [person, reflect.Type.of<LtbPlain>(), named_only]) {
        let type T extends LtbNamed = t else {
            continue;
        };
        names.push(baml.json.to<T>({ "name": ltb_name(t), "tag": "t" }).name);
    }
    assert.equal(names, [ltb_name(person), ltb_name(named_only)])
}

test "an intersection bound checks every interface" {
    assert.equal(ltb_bind_both(reflect.Type.of<LtbPerson>()), "Ada/admin");
    let named_only = reflect.Type.of<LtbNamedOnly>();
    let missed = ltb_bind_both(named_only) catch (e) {
        reflect.errors.TypeMismatch => e.message,
    };
    assert.equal(
        missed,
        "type "
            + ltb_name(named_only)
            + " does not implement interface "
            + ltb_name(reflect.Type.of<LtbTagged>()),
    )
}

test "the bound's members are reachable on values of the bound type" {
    let type T extends LtbGreeter = reflect.Type.of<LtbPerson>();
    let value = baml.json.to<T>({ "name": "Ada", "tag": "admin" });
    assert.equal(value.greet(), "hello, Ada");
    assert.equal(value.name, "Ada")
}

test "a bound is satisfied through requires" {
    // `LtbGreeter requires LtbNamed`, so a `T extends LtbGreeter` is an
    // `LtbNamed` too.
    let type T extends LtbGreeter = reflect.Type.of<LtbPerson>();
    let value = baml.json.to<T>({ "name": "Grace", "tag": "admin" });
    assert.equal(ltb_name_of(value), "Grace")
}

test "a bounded binding fills a bounded generic parameter" {
    let type T extends LtbNamed = reflect.Type.of<LtbNamedOnly>();
    let value = baml.json.to<T>({ "name": "Ada" });
    assert.equal(ltb_name_of<T>(value), "Ada");
    assert.equal(ltb_name_of(value), "Ada")
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
    assert.equal(ltb_increment_held(reflect.Type.of<LtbIntBox>()), "42");
    assert.equal(ltb_increment_held(reflect.Type.of<LtbStringBox>()), "missed")
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
    let foo_class = reflect.class.new(
        "Foo",
        { "a": reflect.Type.of<int>(), "c": reflect.Type.of<int>() },
        implementations = [reflect.interface.implementation<LtbFooInterface>().field("a")],
    );
    let type Foo extends LtbFooInterface = foo_class;
    let out = ltb_extract<Foo>({ "a": 1, "c": 2 });
    assert.equal(out.a, 1)
}

// A session re-binds an earlier `let type` in each later submission with its
// bounds, so `T` keeps their members there.
test "a session binding keeps its bounds in later submissions" {
    let s = reflect.Session.new();
    s.eval(`interface SessionNamed { name string }`);
    s.eval(`class SessionPerson { name string  implements SessionNamed {} }`);
    s.eval(`let type P extends SessionNamed = reflect.Type.of<SessionPerson>();`);
    assert.equal(s.eval<string>(`baml.json.to<P>({ "name": "Ada" }).name`), "Ada")
}

test "a session binding checks its bounds where it is written" {
    let s = reflect.Session.new();
    s.eval(`interface SessionNamed { name string }`);
    let missed = s.eval<string>(
        `let type P extends SessionNamed = reflect.Type.of<int>() else { throw "missed" }; "bound"`,
    ) catch (e) {
        reflect.errors.EvaluationError => e.message,
    };
    assert.equal(missed, "missed")
}
