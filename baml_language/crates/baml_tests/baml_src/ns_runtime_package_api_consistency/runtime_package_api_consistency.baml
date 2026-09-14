// Native coverage for the BEP-066 reflect.Package lookup and enumeration APIs.

function rpac_all_finders_and_enumerators_are_available(pkg: reflect.Package) -> bool {
    pkg.get_enum("root.E") == null
        && pkg.get_interface("root.I") == null
        && pkg.enums().length() == 0
        && pkg.interfaces().length() == 0
}

function rpac_kind_specific_finders_are_available(pkg: reflect.Package) -> bool {
    pkg.get_class("root.E") == null
        && pkg.get_enum("root.E") == null
        && pkg.get_interface("root.I") == null
}

class PurchaseOrder {
    id: string,
}

class OrderedItem {
    sku: string,
}

test "p8_requires_all_precise_type_finders_and_enumerators" {
    let pkg = reflect.Package.compile({ "empty.baml": "" });
    assert.is_true(rpac_all_finders_and_enumerators_are_available(pkg))
}

test "e6_requires_kind_specific_nullable_finders" {
    let pkg = reflect.Package.compile({ "empty.baml": "" });
    assert.is_true(rpac_kind_specific_finders_are_available(pkg))
}

test "package_type_finders_and_enumerators_are_precise_by_kind" {
    let source = `
        class C { value string }
        enum E { A }
        interface I {}
        type Alias = C
        function F() -> null { null }
    `;
    let first = reflect.Package.compile({ "types.baml": source });
    let second = reflect.Package.compile({ "types.baml": source });

    let class_t = first.get_class("root.C") ?? throw "missing class";
    let enum_t = first.get_enum("root.E") ?? throw "missing enum";
    let interface_t = first.get_interface("root.I") ?? throw "missing interface";
    let listed_class = first.classes().get("root.C") ?? throw "class not enumerated";
    let listed_enum = first.enums().get("root.E") ?? throw "enum not enumerated";
    let listed_interface = first.interfaces().get("root.I") ?? throw "interface not enumerated";
    let second_enum = second.get_enum("root.E") ?? throw "missing second enum";
    let second_interface = second.get_interface("root.I") ?? throw "missing second interface";

    assert.is_true(first.get_class("root.E") == null);
    assert.is_true(first.get_enum("root.C") == null);
    assert.is_true(first.get_interface("root.C") == null);
    assert.is_true(first.get_enum("root.I") == null);
    assert.is_true(first.get_class("root.Alias") == null);
    assert.is_true(first.get_enum("root.Missing") == null);
    assert.equal(first.classes().length(), 1);
    assert.equal(first.enums().length(), 1);
    assert.equal(first.interfaces().length(), 1);
    assert.equal(first.functions().length(), 1);
    assert.equal(class_t.as_type(), listed_class.as_type());
    assert.equal(enum_t.as_type(), listed_enum.as_type());
    assert.equal(interface_t.as_type(), listed_interface.as_type());
    assert.equal(enum_t.values()[0].name, "A");
    assert.is_true(enum_t.as_type() != second_enum.as_type());
    assert.is_true(interface_t.as_type() != second_interface.as_type())
}

test "package_class_enumeration_preserves_declaration_order" {
    // The native corpus is one package, so isolate this namespace before
    // comparing the same two declarations the former standalone test saw.
    let current = reflect.Package.current()
        .classes()
        .keys()
        .filter((name) -> {
            name.starts_with("root.runtime_package_api_consistency.")
        })
        .join("|");
    let compiled = reflect.Package
        .compile(
            {
                "types.baml": `
                    class PurchaseOrder { id string }
                    class OrderedItem { sku string }
                `,
            },
        )
        .classes()
        .keys()
        .join("|");
    assert.equal(
        current,
        "root.runtime_package_api_consistency.PurchaseOrder|root.runtime_package_api_consistency.OrderedItem",
    );
    assert.equal(compiled, "root.PurchaseOrder|root.OrderedItem")
}
