// What a `with_types` call exports does not depend on the order of its keys.
// A mounted type exports every declaration it reaches under that
// declaration's own name, so a call that also names such a declaration by key
// states one export twice, and that is one export whichever the call met
// first. A name given to two DIFFERENT declarations is refused either way.

function person_and_address() -> reflect.class.Type[] {
    let address = reflect.class.new("Address", { "city": reflect.Type.of<string>() });
    let person = reflect.class.new("Person", { "home": address.as_type() });
    [person, address]
}

function empty_base() -> reflect.Package {
    reflect.Package.compile({ "base.baml": "" })
}

// A consumer that writes both names, and the field between them.
function consumer_of(view: reflect.Package) -> reflect.Package {
    reflect.Package.compile(
        {
            "consumer.baml": `
                function home_of(person: app.Person) -> app.Address { person.home }
                function same() -> bool {
                    reflect.Type.of<app.Address>() == reflect.Type.of_value(home_of(app.Person { home: app.Address { city: "x" } }))
                }
            `,
        },
        packages = { "app": view },
    )
}

test "a key may name a declaration an earlier key reached" {
    let types = person_and_address();
    let view = empty_base().with_types({ "Person": types[0], "Address": types[1] });
    assert.equal(view.classes().keys().join("|"), "root.Person|root.Address");
    let same = consumer_of(view).get_function<() -> bool>("root.same") ?? throw "missing same";
    assert.is_true(same())
}

test "a key may name a declaration a later key reaches" {
    let types = person_and_address();
    let view = empty_base().with_types({ "Address": types[1], "Person": types[0] });
    assert.equal(view.classes().keys().join("|"), "root.Address|root.Person");
    let same = consumer_of(view).get_function<() -> bool>("root.same") ?? throw "missing same";
    assert.is_true(same())
}

test "the reaching key alone exports both" {
    let types = person_and_address();
    let view = empty_base().with_types({ "Person": types[0] });
    assert.equal(view.classes().keys().join("|"), "root.Person|root.Address");
    let same = consumer_of(view).get_function<() -> bool>("root.same") ?? throw "missing same";
    assert.is_true(same())
}

// What a call is refused with, or "accepted".
function refused_on(
    base: reflect.Package,
    types: map<string, reflect.Type | reflect.TypeView>,
) -> string {
    let view = base.with_types(types) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + " " + e.diagnostics[0].message
    };
    if (view is string) {
        view
    } else {
        "accepted"
    }
}

// Another class that calls itself `Address`: the name `Address` cannot be
// both the one `Person` reaches and this one.
test "a name given to two declarations is refused in either order" {
    let types = person_and_address();
    let other = reflect.class.new("Address", { "street": reflect.Type.of<string>() });
    assert.equal(
        refused_on(empty_base(), { "Person": types[0], "Address": other }),
        "E0011 duplicate exported type name `Address`",
    );
    assert.equal(
        refused_on(empty_base(), { "Address": other, "Person": types[0] }),
        "E0011 duplicate exported type name `Address`",
    )
}

// The base's own exports are not the call's to restate: a key is refused for
// a name the base already exports, even as the same declaration, in either
// order.
test "a key is refused for a name the base exports in either order" {
    let types = person_and_address();
    let base = empty_base().with_types({ "Address": types[1] });
    assert.equal(
        refused_on(base, { "Person": types[0], "Address": types[1] }),
        "E0011 duplicate exported type name `Address`",
    );
    assert.equal(
        refused_on(base, { "Address": types[1], "Person": types[0] }),
        "E0011 duplicate exported type name `Address`",
    );
    // Reaching it is no restatement: `Person` alone mounts over the base,
    // and adds only itself.
    let view = base.with_types({ "Person": types[0] });
    assert.equal(view.classes().keys().join("|"), "root.Person|root.Address")
}
