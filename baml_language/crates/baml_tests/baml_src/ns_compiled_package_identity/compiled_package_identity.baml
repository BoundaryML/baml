// BEP-066 I-2: declarations in a compiled Package have one created-once
// identity across every access path.

test "compiled_package_declarations_share_one_identity_across_all_access_paths" {
    let pkg = reflect.Package.compile(
        {
            "pkg.baml": `
                class Item { value int }
                enum State { Ready }
                function ReflectedItem() -> reflect.Type { reflect.Type.of<Item>() }
                function ReflectedState() -> reflect.Type { reflect.Type.of<State>() }
            `,
        },
    );

    let item = pkg.get_class("root.Item") ?? throw "missing Item";
    let state = pkg.get_enum("root.State") ?? throw "missing State";
    let listed_item = pkg.classes().get("root.Item") ?? throw "Item not enumerated";
    let listed_state = pkg.enums().get("root.State") ?? throw "State not enumerated";
    let reflected_item = pkg.get_function<() -> reflect.Type>("root.ReflectedItem") ?? throw "missing ReflectedItem";
    let reflected_state = pkg.get_function<() -> reflect.Type>("root.ReflectedState") ?? throw "missing ReflectedState";

    assert.equal(item.as_type(), reflected_item());
    assert.equal(item.as_type(), listed_item.as_type());
    assert.equal(state.as_type(), reflected_state());
    assert.equal(state.as_type(), listed_state.as_type())
}
