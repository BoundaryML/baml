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

interface McapDriver<Input> {
    type Output
    type Error

    function drive(self, input: Input) -> Self.Output throws Self.Error
}

class McapTask<T> {
    value: T

    function drive<D extends McapDriver<McapTask<T>>>(self, driver: D) -> D.Output throws D.Error {
        driver.drive(self)
    }
}

class McapIntDriver {
    implements McapDriver<McapTask<int>> {
        type Output = string
        type Error = never

        function drive(self, input: McapTask<int>) -> string {
            if (input.value == 42) { "ok" } else { "wrong" }
        }
    }
}

test "method_associated_projection_preserves_class_generic" {
    let task = McapTask<int> { value: 42 }
    assert.equal(task.drive(McapIntDriver {}), "ok")
}

interface AemEq<T> {
    function get(self) -> T throws never
}

interface AemEcho<T> {
    function echo<U, V extends AemEq<U>>(self, value: V) -> U throws never
}

class AemValue<T> {
    value: T

    implements AemEq<T> {
        function get(self) -> T {
            self.value
        }
    }
}

class AemEchoer {
    implements AemEcho<int> {
        function echo<A, B extends AemEq<A>>(self, value: B) -> A {
            value.get()
        }
    }
}

function aem_accept_echo(value: AemEcho<int>) -> int {
    return value.echo(AemValue<int> { value: 42 })
}

test "alpha_equivalent_method_generics_and_bounds_run" {
    assert.equal(aem_accept_echo(AemEchoer {}), 42)
}
