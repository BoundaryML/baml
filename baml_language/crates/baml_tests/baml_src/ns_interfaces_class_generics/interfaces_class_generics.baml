// Interface-related behavior on class generic parameters.
//
// Test bodies use top-level helper functions to work around a local-boxing
// VM bug in test blocks; string results are asserted with assert.equal.

interface Named {
    name: string
}

// ─── class generic bound: member access through the bound in a method ────────

class Dog {
    name: string
    implements Named {}
}

class Box<T extends Named> {
    value: T

    function label(self) -> string {
        return self.value.name
    }
}

function class_generic_bound_member_access_fn() -> string {
    return Box<Dog> { value: Dog { name: "Rex" } }.label()
}

test "class_generic_bound_member_access_runs_in_class_method" {
    assert.equal(class_generic_bound_member_access_fn(), "Rex")
}

// ─── function generic bound: member access through the bound ─────────────────

class Person {
    name: string
    implements Named {}
}

function get_name<T extends Named>(value: T) -> string {
    return value.name
}

function function_class_generic_bound_member_access_fn() -> string {
    return get_name(Person { name: "Ada" })
}

test "function_class_generic_bound_member_access_runs" {
    assert.equal(function_class_generic_bound_member_access_fn(), "Ada")
}
