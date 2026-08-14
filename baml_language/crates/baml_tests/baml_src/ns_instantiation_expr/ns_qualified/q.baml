// A generic function in the nested namespace `instantiation_expr.qualified`,
// to exercise multi-segment qualified instantiation values
// (`user.instantiation_expr.qualified.q_describe<T>`).
function q_describe<T>() -> string { reflect.type_of<T>().to_string() }
function q_id<T>(x: T) -> T { x }
