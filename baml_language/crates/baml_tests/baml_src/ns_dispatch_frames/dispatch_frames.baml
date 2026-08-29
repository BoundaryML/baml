// Statically-resolved interface-method calls carry the callee's OWNER frame
// on the resolution (`[owner ++ own]`): the impl's generic bindings for a
// provided method, `[Self, iface args..]` for an adopted default. These pin
// the shapes that previously routed through the interface's method ref and
// ran the default body with an empty frame (a runtime trap).
//
// Test bodies live in top-level helper functions (test-block local-boxing VM
// bug — locals holding instances misbehave inside test blocks).

interface Greeter {
    function greet(self) -> string throws never {
        "default"
    }
}

class Adopts {
    implements Greeter {}
}

/// A concrete receiver adopting an interface default: the static call reaches
/// the default body with `[Self = Adopts]` seeded, so its `Self`-typed frame
/// references realize instead of trapping.
function call_adopted_default() -> string throws never {
    let a = Adopts {};
    a.greet()
}

interface Tagged {
    function tag(self) -> string throws never
}

class Box<T> {
    value T
}

implements<T> Tagged for Box<T> {
    function tag(self) -> string throws never {
        reflect.Type.of<T>().to_string()
    }
}

/// A GENERIC free impl's override called statically: the resolution carries
/// the impl's bindings (`T = int`), which the body observes by reflection.
function call_generic_override() -> string throws never {
    let b = Box { value: 1 };
    b.tag()
}

interface Conv<T> {
    function describe(self) -> string throws never {
        reflect.Type.of<T>().to_string()
    }
}

class Pinner {
    implements Conv<int> {}
}

/// A default body on a GENERIC interface, adopted by an impl that pins the
/// argument: the carried frame is `[Self, int]`, and the body observes the
/// interface argument at its slot.
function call_generic_interface_default() -> string throws never {
    let c = Pinner {};
    c.describe()
}

test "static call reaches an inherited default" {
    assert.equal(call_adopted_default(), "default")
}

test "static call reaches a generic impl override with its bindings" {
    assert.equal(call_generic_override(), "int")
}

test "a generic interface default observes its pinned argument" {
    assert.equal(call_generic_interface_default(), "int")
}
