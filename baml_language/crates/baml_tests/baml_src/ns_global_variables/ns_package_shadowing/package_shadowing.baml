// A global spelled like a package shadows it the way any item of the
// namespace does (#4543, see ns_reflect_shadowing): it claims the head of
// `reflect.x`, and the package is unreachable by that name from here. A
// type's statics are not a package, so they still win over a global named
// like the type (`float.nan` beside `const float` in global_variables.baml).

class Settings {
    retries int
}

const reflect = Settings { retries: 4 };

function reads_the_global() -> int {
    reflect.retries
}

test "a global named like a package shadows it" {
    assert.equal(reads_the_global(), 4)
}
