// The interface view is written like a call: `value.upcast<Interface>()`.
// The parentheses are part of the expression and hold nothing.
//
// - `value.upcast<Interface>` without them reports once and says to add `()`.
// - Arguments between them report once.
//
// Either way the view is still parsed and checked as one, so nothing
// cascades. `upcast` is the operator only before a `<` that opens type
// arguments; anywhere else it is an ordinary member name.

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
        function label(self) -> string throws never {
            "Dr. " + self.upcast<Named>.name
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

// `obj.upcast<I>.m()`
function method_call(p: Person) -> string {
    p.upcast<Named>.label()
}

function field_access(p: Person) -> string {
    p.upcast<Named>.name
}

// The replacement carries the written type arguments, nested ones included.
function nested_generic(b: IntBox) -> int {
    b.upcast<Boxed<int>>.get()
}

// A base that is not a path.
function computed_base() -> string {
    make().upcast<Titled>.label()
}

// The bare view, with nothing after it and with an operator after it.
function bare_view(p: Person) -> Named {
    p.upcast<Named>
}

function before_an_operator(b: IntBox) -> bool {
    b.upcast<Boxed<int>> == b.upcast<Boxed<int>>()
}

// Arguments, positional and named.
function with_an_argument(p: Person) -> string {
    p.upcast<Named>(1).label()
}

function with_a_named_argument(p: Person) -> string {
    p.upcast<Named>(strict = true).label()
}

// The accepted spelling.
function accepted_spelling(p: Person, b: IntBox) -> string {
    let view: Named = p.upcast<Named>();
    view.label() + p.upcast<Titled>().label() + self_key(b)
}

function self_key(b: IntBox) -> string {
    if (b.upcast<Keyed<Key = int>>().key() == b.upcast<Boxed<int>>().get()) {
        "same"
    } else {
        "different"
    }
}

// `upcast` with no type argument list after it is an ordinary member: a
// field read, a method call, and the left side of a comparison.
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

function plain_members(l: Lift, r: Lifter) -> bool {
    l.upcast + l.twice() + r.upcast() < l.upcast && l.upcast < r.upcast()
}

// With type arguments it is always the view, even on a class that declares
// a generic method called `upcast`: that method is still reached when its
// type argument is inferred.
class Shadow {
    function upcast<T>(self, value: T) -> T {
        value
    }

    implements Titled {
        function label(self) -> string throws never {
            "shadow"
        }
    }
}

function view_wins(s: Shadow) -> string {
    let viewed: Titled = s.upcast<Titled>();
    let called: int = s.upcast(1);
    viewed.label()
}
