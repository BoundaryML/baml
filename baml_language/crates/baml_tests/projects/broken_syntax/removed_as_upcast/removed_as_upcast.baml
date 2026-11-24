// `value.upcast<Interface>` replaced `value.as<Interface>`. Each use of the
// removed spelling reports once (E0098) with the replacement spelled out, and
// recovers without cascading: the view is still parsed and checked as one.
// `as` keeps its other meanings, the field link in an `implements` block and
// the qualified projection `(Type as Interface).Assoc`, and `upcast` stays a
// plain member name wherever no `<` follows it.

interface Named {
    name: string,
    function label(self) -> string throws never
}

interface Titled {
    function label(self) -> string throws never
}

interface Boxed<T> {
    function get(self) -> T throws never
}

interface Keyed {
    type Key
    function key(self) -> Self.Key throws never
}

class Person {
    full_name: string,

    implements Named {
        name as full_name

        function label(self) -> string throws never {
            self.full_name
        }
    }

    implements Titled {
        // `self.as<I>.field`
        function label(self) -> string throws never {
            "Dr. " + self.as<Named>.name
        }
    }
}

class IntBox {
    value: int,

    implements Boxed<int> {
        function get(self) -> int throws never {
            self.value
        }
    }

    implements Keyed {
        type Key = int

        function key(self) -> int throws never {
            self.value
        }
    }
}

function make() -> Person {
    Person { full_name: "Ada" }
}

// `obj.as<I>.m()`
function method_call(p: Person) -> string {
    p.as<Named>.label()
}

function field_access(p: Person) -> string {
    p.as<Named>.name
}

// The replacement carries the written type arguments, nested ones included.
function nested_generic(b: IntBox) -> int {
    b.as<Boxed<int>>.get()
}

// A base that is not a path.
function computed_base() -> string {
    make().as<Titled>.label()
}

// The bare view, with nothing after it.
function bare_view(p: Person) -> Named {
    p.as<Named>
}

// The new spelling, and the two other uses of `as`, are unaffected.
function new_spelling(p: Person) -> string {
    p.upcast<Named>.label() + p.upcast<Titled>.label()
}

function projected(b: IntBox) -> (IntBox as Keyed).Key {
    b.upcast<Keyed<Key = int>>.key() + b.upcast<Boxed<int>>.get()
}

// `upcast` with no `<` after it is an ordinary member.
class Lift {
    upcast: int,

    function twice(self) -> int {
        self.upcast + self.upcast
    }
}

class Lifter {
    function upcast(self) -> int {
        1
    }
}

function plain_members(l: Lift, r: Lifter) -> int {
    l.upcast + l.twice() + r.upcast()
}
