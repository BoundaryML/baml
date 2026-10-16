// `reflect` is an ordinary root package (#4543), so a user-defined name
// spelled `reflect` shadows it exactly the way it shadows any other package
// name: an item of this namespace claims the head of `reflect.x`, and the
// package is unreachable by that name from here (the rejected side is
// `diagnostic_errors/local_item_shadows_package`).

class reflect {
    label string

    function describe(self) -> string {
        return "user-" + self.label
    }

    // Spelled like the package's free function `reflect.signature`.
    function signature(self) -> string {
        return "member-" + self.label
    }
}

// A user class named `reflect` wins over the package: construction and member
// access on it hit the class.
function user_class_shadows_package() -> string {
    let r = reflect { label: "shadow" }
    return r.describe()
}

// Type-rooted path access on the user class also wins: `reflect.describe` is
// the class's unbound method, not a package lookup.
function unbound_user_method() -> string {
    let m = reflect.describe
    return m(reflect { label: "x" })
}

// A local named `reflect` shadows both the class and the package.
function local_shadows_package() -> int {
    let reflect = 1
    return reflect + 1
}

// The class shadows the package entirely: `reflect.signature` is the
// class's method, not the package's function of that name.
function class_member_over_package_function() -> string {
    let s = reflect.signature
    return s(reflect { label: "sig" })
}

test "user class shadows the package" {
    assert.equal(user_class_shadows_package(), "user-shadow")
}

test "unbound user method over the package" {
    assert.equal(unbound_user_method(), "user-x")
}

test "local shadows the package" {
    assert.equal(local_shadows_package(), 2)
}

test "class member over package function" {
    assert.equal(class_member_over_package_function(), "member-sig")
}
