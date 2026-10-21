// An interface fixes a method's required slots; an implementation may declare
// extra optional parameters. Every call site therefore carries the argument
// layout it was checked against, and dispatch maps that layout onto whichever
// implementation it reaches — an open-world virtual call, a captured method
// narrowed to a plainer function type, or a native higher-order builtin.

interface OfpFormatter {
    function format(self, value: string) -> string throws never
}

class OfpFancy {
    implements OfpFormatter {
        function format(
            self,
            value: string,
            extra: string? = null,
            prefix: string? = null,
        ) -> string throws never {
            `${prefix ?? "["}${value}${extra ?? ""}]`
        }
    }
}

function ofp_virtual_call(f: OfpFormatter) -> string {
    f.format("Ada")
}

test "virtual_call_omits_the_implementation_extras" {
    assert.equal(ofp_virtual_call(OfpFancy {}), "[Ada]")
}

test "static_call_names_the_implementation_optional" {
    assert.equal(OfpFancy {}.format("Ada", prefix = "<"), "<Ada]")
}

function ofp_narrowed_method_value(f: OfpFormatter) -> string[] {
    let render: (string) -> string throws never = f.format;
    ["Ada", "Bob"].map(render)
}

test "captured_interface_method_dispatches_through_a_native_builtin" {
    assert.equal(ofp_narrowed_method_value(OfpFancy {}), ["[Ada]", "[Bob]"])
}

function ofp_class_method_value() -> string[] {
    let fancy = OfpFancy {};
    ["Ada", "Bob"].map(fancy.format)
}

test "captured_class_method_dispatches_through_a_native_builtin" {
    assert.equal(ofp_class_method_value(), ["[Ada]", "[Bob]"])
}

function ofp_indirect_named_call() -> string {
    let fancy = OfpFancy {};
    // Drops `extra`, keeps `prefix`: the named slot must land on the
    // implementation's `prefix`, not its `extra`.
    let render: (value: string, prefix?: string) -> string throws never = fancy.format;
    render("Ada", prefix = "<")
}

test "indirect_call_keeps_named_optional_slot" {
    assert.equal(ofp_indirect_named_call(), "<Ada]")
}
