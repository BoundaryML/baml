// pending: literal-context: a generic constructor ignores its expected class arguments (00-examples section 6)
// The expected result type contextualizes a generic constructor's
// container-valued field: `Box<(int | string)[]>` determines `T` before
// `[1]` is checked, so the literal is built as `(int | string)[]`.
class LcBox<T> {
    value T
}

function box_array() -> LcBox<(int | string)[]> throws never {
    LcBox { value: [1] }
                // ^^^ (int | string)[]
}

function box_map() -> LcBox<map<string, int | string>> throws never {
    LcBox { value: {key: 1} }
                // ^^^^^^^^ map<string, int | string>
}

// Controls: a scalar field, and a generic object inside a list literal.
function box_scalar() -> LcBox<int | string> throws never {
    LcBox { value: 1 }
//  ^^^^^^^^^^^^^^^^^^ user.LcBox<int | string>
}

function box_in_list() -> LcBox<int | string>[] throws never {
    [LcBox { value: 1 }]
//  ^^^^^^^^^^^^^^^^^^^^ user.LcBox<int | string>[]
}
