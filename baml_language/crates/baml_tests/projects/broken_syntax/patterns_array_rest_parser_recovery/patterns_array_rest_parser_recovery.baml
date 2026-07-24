// Array rest-binding torture cases (B-531), parser half: malformed rests
// must error without derailing the rest of the file. The semantic
// (parses-clean) cases live in patterns_array_rest_binding.

// Two rests, both carrying bindings.
function double_rest_bindings(xs: int[]) -> int {
    match (xs) {
        [..let a, ..let b] => a.length() + b.length(),
        _ => 0
    }
}

// Spread token (`...`) is expression syntax, not a rest pattern.
function spread_token_is_not_rest(xs: int[]) -> int {
    match (xs) {
        [...let r] => r.length(),
        _ => 0
    }
}

// Rest binding missing its name.
function rest_missing_binding_name(xs: int[]) -> int {
    match (xs) {
        [..let] => 1,
        _ => 0
    }
}

// Ascription directly on bare `..`.
function rest_bare_ascription(xs: int[]) -> int {
    match (xs) {
        [.. : int[]] => 1,
        _ => 0
    }
}

// Wildcard rest cannot carry an ascription.
function rest_wildcard_ascription(xs: int[]) -> int {
    match (xs) {
        [.._ : int[]] => 1,
        _ => 0
    }
}

// Double rest nested inside a rest sub-pattern.
function nested_double_rest(xs: int[]) -> int {
    match (xs) {
        [..[.., ..]] => 1,
        _ => 0
    }
}
