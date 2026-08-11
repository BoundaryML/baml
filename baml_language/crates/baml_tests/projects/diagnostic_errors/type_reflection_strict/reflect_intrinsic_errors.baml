// Negative cases for the `reflect.type_of` compiler intrinsic itself.
// These complement `strict_normalization.baml` (which exercises Ty::Type
// strictness via `baml.prompt.get_return_type`) by hitting the intrinsic path
// directly.

class User { name string }

// Direct strictness via the intrinsic: a `type` value cannot be assigned
// to a non-`type` slot. Validates Phase 2's strictness all the way through
// Phase 4's `LoadType` lowering.
function reflect_type_to_int() -> int {
    let x: int = reflect.type_of<User>();
    return x;
}

// Unknown type referenced inside `reflect.type_of<...>`.
function unknown_type_in_reflect() -> type {
    reflect.type_of<NonExistent>()
}

// Arity mismatch on the intrinsic: it declares one type parameter, caller
// passes two. Validates Phase 1's `WrongTypeArgArity` against the new
// builtin.
function too_many_type_args() -> type {
    reflect.type_of<int, string>()
}
