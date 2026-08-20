// Every spelling reflection answered to before it became a root package
// (#4543). Each one now reports where to write instead, rather than reading as
// an ordinary typo.

// The type-value API moved off the bare `type.` prefix.
function old_type_of() -> reflect.Type {
    type.of<int>()
}

// A bare `type` annotation used to name a runtime type value.
function old_type_annotation() -> type {
    reflect.Type.of<int>()
}

// Reflection used to live inside the `baml` stdlib — in type position...
function old_qualified_type(p: baml.reflect.Package) -> string {
    "unused"
}

// ...and in expression position.
function old_qualified_call() -> string {
    baml.reflect.Type.of<int>().to_string()
}

// None of this touches the `type` keyword. An alias, a scoped type binding,
// and an ordinary binding that happens to be called `type` all still compile,
// so none of them appears in the diagnostics below.
type StillAnAlias = int | string

function type_keyword_still_works(t: reflect.Type) -> string {
    type T = unreflect(t)
    let type = reflect.Type.of<T>();
    type.to_string()
}

function alias_still_resolves(v: StillAnAlias) -> string {
    v.to_string()
}
