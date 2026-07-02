// B-633: a bare `let x: T` arm over a union scrutinee `T | <concrete>` must
// NOT claim the concrete member. A `let x: T` pattern targets only the
// union's `T` member — never `null`/`string` — because when `T` is
// instantiated to a non-nullable/non-string type that arm cannot match the
// concrete value. So these matches are non-exhaustive (E0062) and `null`
// (or `string`) must not silently bind into a non-nullable `T` at runtime.
//
// Mirrors the concrete control `match (v: int?) { let x: int => x }`, which
// has always (correctly) reported E0062 "missing: null".

class Box<T> {
    items T[]
}

// (a) `.pop()` returns `T?`; the lone `let x: T` arm leaves `null` uncovered.
function bad_pop<T>(b: Box<T>) -> T {
    match (b.items.pop()) {
        let x: T => x,
    }
}

// (b) `.at(0)` shares the identical `T?` shape and root cause as `.pop()`.
function bad_at<T>(b: Box<T>) -> T {
    match (b.items.at(0)) {
        let x: T => x,
    }
}

// (c) A bare `T?` scrutinee: `let x: T` must not claim the `null` member.
function opt<T>(v: T?) -> T {
    match (v) {
        let x: T => x,
    }
}

// (d) `T | string`: `let x: T` must not claim the concrete `string` member.
function uni<T>(v: T | string) -> T {
    match (v) {
        let x: T => x,
    }
}
