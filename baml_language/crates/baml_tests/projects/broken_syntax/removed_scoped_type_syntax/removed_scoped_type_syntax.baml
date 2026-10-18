// `let type T = e;` replaced the body-level `type T = unreflect(e)`
// statement, body-level `type T = <type>` aliases, and `unreflect(…)` in
// every other position. Each removed spelling reports once (E0098) with what
// to write instead, and recovers without cascading: a removed statement still
// binds `T` for the rest of its block. (Kept out of `.rewrite/rewrite.py`.)

class Holder<T> {
    label string

    function new(label: string) -> Holder<T> throws never {
        Holder<T> { label: label }
    }
}

class Wrapper<T> { inner T }

function ident<T>(v: T) -> T throws never { v }

// The removed statement names its `let type` replacement; `T` stays bound.
function removed_statement(t: reflect.Type) -> reflect.Type {
    type T = unreflect(t)
    reflect.Type.of<T>()
}

// A body-level alias of a static type binds through `reflect.Type.of` now.
// An `unreflect` inside the alias is covered by the statement's report.
function removed_body_aliases(t: reflect.Type) -> int {
    type S = Wrapper<int>
    type H = Wrapper<_>
    type N = Wrapper<unreflect(t)>
    1
}

// Body positions: a call slot, a generic-apply value, a class literal, a
// static constructor, an annotation, an `is` pattern, a `match` arm, and
// `reflect.Type.of`.
function body_positions(t: reflect.Type) -> int {
    let call = ident<unreflect(t)>(1)
    let applied = ident<unreflect(t)>
    let literal = Holder<unreflect(t)> { label: "h" }
    let constructed = Holder<unreflect(t)>.new("h")
    let annotated: Wrapper<unreflect(t)>? = null
    let tested = 1 is unreflect(t)
    let arm = match (1) {
        unreflect(t) => 1,
        _ => 0,
    }
    let reflected = reflect.Type.of<unreflect(t)>()
    0
}

// Item signatures: a field, an `implements` argument, a parameter, a return
// type, a bound, and a top-level alias.
interface Marker<T> {}

class FieldSite {
    value Wrapper<unreflect(reflect.Type.of<string>())>
}

class ImplementsSite {
    implements Marker<unreflect(reflect.Type.of<string>())> {}
}

function signature_site(
    value: Wrapper<unreflect(reflect.Type.of<string>())>
) -> Wrapper<unreflect(reflect.Type.of<string>())> {
    value
}

function bound_site<T extends Wrapper<unreflect(reflect.Type.of<string>())>>() -> null { null }

type AliasSite = Wrapper<unreflect(reflect.Type.of<string>())>

// A runtime type binds for the rest of a block, and a file has none.
let type TopLevel = reflect.Type.of<string>();
