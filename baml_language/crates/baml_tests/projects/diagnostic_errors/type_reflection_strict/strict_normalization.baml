// Phase 2 of the runtime-type-reflection plan tightens `Ty::Type` so that
// a `type` value is only compatible with `type` itself. Previously,
// `Ty::Type` normalized to `StructuralTy::Unknown` and was bidirectionally
// compatible with every type, silently swallowing real errors.

// Negative case: assigning a `type` value to `int` must now diagnose.
function assign_type_to_int(function_name: string) -> int {
    let x: int = ai.internal.get_return_type(function_name);
    return x;
}

// Negative case: passing a `type` value where a string is expected must diagnose.
function pass_type_as_string(function_name: string) -> string {
    let t = ai.internal.get_return_type(function_name);
    return t;
}

// Positive case: `type`-to-`type` continues to type-check cleanly.
function type_to_type(function_name: string) -> reflect.Type {
    let t: reflect.Type = ai.internal.get_return_type(function_name);
    return t;
}
