// Test `type` as a type annotation in various positions

// In a let binding
function test_let_binding(function_name: string) -> string {
    let t: reflect.Type = ai.internal.get_return_type(function_name);
    return "ok";
}

// As a function parameter type
function test_param(t: reflect.Type) -> string {
    return "ok";
}

// As a function return type
function test_return_type(function_name: string) -> reflect.Type {
    return ai.internal.get_return_type(function_name);
}

// Optional type
function test_optional(function_name: string) -> string {
    let t: reflect.Type? = null;
    return "ok";
}

// List of types
function test_list(function_name: string) -> string {
    let ts: reflect.Type[] = [];
    return "ok";
}

// Union with type
function test_union(function_name: string) -> string {
    let t: reflect.Type | string = "hello";
    return "ok";
}

// Map value
function test_map(function_name: string) -> string {
    let m: map<string, reflect.Type> = {};
    return "ok";
}
