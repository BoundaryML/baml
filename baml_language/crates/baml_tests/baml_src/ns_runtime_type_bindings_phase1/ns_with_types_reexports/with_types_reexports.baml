// A `with_types` view is a package that re-exports its base and adds the
// given names. A consumer that mounts the base and the view together sees
// one declaration under both spellings; one that mounts the view alone
// reaches the base through it; the base's functions run in the base.

function base_package() -> reflect.Package throws unknown {
    reflect.Package.compile(
        {
            "base.baml": `
                class Item { value int }

                function make(value: int) -> Item { Item { value: value } }

                function home() -> string {
                    reflect.Package.current().classes().keys().join("|")
                }
            `,
        },
    )
}

test "a base and its view are one package to a consumer" {
    let base = base_package();
    let extra = reflect.class.new("Extra", { "tag": reflect.Type.of<string>() });
    let view = base.with_types({ "Extra": extra });
    let consumer = reflect.Package.compile(
        {
            "consumer.baml": `
                function through(item: base.Item) -> app.Item { item }
                function back(item: app.Item) -> base.Item { item }
                function same() -> bool {
                    reflect.Type.of<base.Item>() == reflect.Type.of<app.Item>()
                }
                function tagged() -> app.Extra { app.Extra { tag: "t" } }
            `,
        },
        packages = { "base": base, "app": view },
    );
    let same = consumer.get_function<() -> bool>("root.same") ?? throw "missing same";
    assert.is_true(same());
    type Item = unreflect((base.get_class("root.Item") ?? throw "missing Item").as_type())
    let through = consumer.get_function<(Item) -> Item>("root.through") ?? throw "missing through";
    let make = base.get_function<(int) -> Item>("root.make") ?? throw "missing make";
    let item = through(make(3));
    assert.equal(reflect.Type.of_value(item), reflect.Type.of<Item>());
    type Extra = unreflect(extra.as_type())
    let tagged = consumer.get_function<() -> Extra>("root.tagged") ?? throw "missing tagged";
    assert.equal(reflect.Type.of_value(tagged()), extra.as_type())
}

test "a view alone reaches its base, and the base's functions run in the base" {
    let base = base_package();
    let view = base.with_types({ "Extra": reflect.class.new("Extra", { "tag": reflect.Type.of<string>() }) });
    let consumer = reflect.Package.compile(
        {
            "consumer.baml": `
                function build() -> app.Item { app.make(7) }
                function value(item: app.Item) -> int { item.value }
                function home() -> string { app.home() }
            `,
        },
        packages = { "app": view },
    );
    type Item = unreflect((base.get_class("root.Item") ?? throw "missing Item").as_type())
    let build = consumer.get_function<() -> Item>("root.build") ?? throw "missing build";
    let item = build();
    assert.equal(reflect.Type.of_value(item), reflect.Type.of<Item>());
    let value = consumer.get_function<(Item) -> int>("root.value") ?? throw "missing value";
    assert.equal(value(item), 7);
    let home = consumer.get_function<() -> string>("root.home") ?? throw "missing home";
    assert.equal(home(), "root.Item");
    // The view answers for its base: the same function, the same class.
    let make = view.get_function<(int) -> Item>("root.make") ?? throw "the view re-exports make";
    assert.equal(reflect.Type.of_value(make(1)), reflect.Type.of<Item>());
    assert.equal((view.get_class("root.Item") ?? throw "missing Item").as_type(), reflect.Type.of<Item>());
    assert.is_true(view.functions().get("root.make") != null)
}

test "a view of a view re-exports every layer" {
    let base = base_package();
    let first = base.with_types({ "First": reflect.class.new("First", { "a": reflect.Type.of<int>() }) });
    let second = first.with_types({ "Second": reflect.class.new("Second", { "b": reflect.Type.of<int>() }) });
    let consumer = reflect.Package.compile(
        {
            "consumer.baml": `
                function all() -> int {
                    let item = app.make(1);
                    let first = app.First { a: 2 };
                    let second = app.Second { b: 3 };
                    item.value + first.a + second.b
                }
            `,
        },
        packages = { "app": second },
    );
    let all = consumer.get_function<() -> int>("root.all") ?? throw "missing all";
    assert.equal(all(), 6)
}

test "a minted class reached through two views is one type" {
    let minted = reflect.class.new("Minted", { "n": reflect.Type.of<int>() });
    let one = reflect.Package.compile({ "one.baml": "" }).with_types({ "Minted": minted });
    let two = reflect.Package.compile({ "two.baml": "" }).with_types({ "Minted": minted });
    let consumer = reflect.Package.compile(
        {
            "consumer.baml": `
                function across(value: one.Minted) -> two.Minted { value }
                function same() -> bool {
                    reflect.Type.of<one.Minted>() == reflect.Type.of<two.Minted>()
                }
            `,
        },
        packages = { "one": one, "two": two },
    );
    let same = consumer.get_function<() -> bool>("root.same") ?? throw "missing same";
    assert.is_true(same())
}

test "a renamed re-export is the same declaration" {
    let base = base_package();
    let view = base.with_types({ "Twin": base.get_class("root.Item") ?? throw "missing Item" });
    let consumer = reflect.Package.compile(
        {
            "consumer.baml": `
                function same() -> bool {
                    reflect.Type.of<app.Twin>() == reflect.Type.of<app.Item>()
                }
                function twin() -> app.Twin { app.make(9) }
            `,
        },
        packages = { "app": view },
    );
    let same = consumer.get_function<() -> bool>("root.same") ?? throw "missing same";
    assert.is_true(same());
    assert.equal(
        (view.get_class("root.Twin") ?? throw "missing Twin").as_type(),
        (base.get_class("root.Item") ?? throw "missing Item").as_type(),
    )
}

test "with_types declares an alias for a type that is not a bare declaration" {
    let base = base_package();
    let view = base.with_types({ "Ints": reflect.Type.of<int[]>() });
    let consumer = reflect.Package.compile(
        {
            "consumer.baml": `
                function total(values: app.Ints) -> int {
                    let sum = 0;
                    for (let v in values) { sum = sum + v }
                    sum
                }
            `,
        },
        packages = { "app": view },
    );
    let total = consumer.get_function<(int[]) -> int>("root.total") ?? throw "missing total";
    assert.equal(total([1, 2, 3]), 6);
    // An alias is not a class.
    assert.is_true(view.get_class("root.Ints") == null)
}

class HostRecord {
    name string
}

test "with_types re-exports a class of the host program" {
    let base = base_package();
    let view = base.with_types({ "Record": reflect.Type.of<HostRecord>() });
    let consumer = reflect.Package.compile(
        {
            "consumer.baml": `
                function name(record: app.Record) -> string { record.name }
            `,
        },
        packages = { "app": view },
    );
    let name = consumer.get_function<(HostRecord) -> string>("root.name") ?? throw "missing name";
    assert.equal(name(HostRecord { name: "host" }), "host")
}
