// Test `type` as a type annotation in various positions

// In a let binding
function test_let_binding(function_name: string) -> string {
    let t: type = baml.llm.get_return_type(function_name);
    return "ok";
}

// As a function parameter type
function test_param(t: type) -> string {
    return "ok";
}

// As a function return type
function test_return_type(function_name: string) -> type {
    return baml.llm.get_return_type(function_name);
}

// Optional type
function test_optional(function_name: string) -> string {
    let t: type? = null;
    return "ok";
}

// List of types
function test_list(function_name: string) -> string {
    let ts: type[] = [];
    return "ok";
}

// Union with type
function test_union(function_name: string) -> string {
    let t: type | string = "hello";
    return "ok";
}

// Map value
function test_map(function_name: string) -> string {
    let m: map<string, type> = {};
    return "ok";
}
