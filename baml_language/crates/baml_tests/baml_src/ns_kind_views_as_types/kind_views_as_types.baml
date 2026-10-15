// Kind views (reflect.Class, reflect.Enum, ...) go anywhere a type does:
// every reflection API that takes a type takes `reflect.TypeView`, which
// `reflect.Type` and every kind implement, and the shared type operations are
// `TypeView` methods. None of these need `.as_type()`.

/// An item of a tag list.
class Item {
    label: string,

    @@alias("Entry")
    @@description("One item")
}

class Note {
    body: string,
}

enum Mood {
    Happy,
    Sad,
}

class Box<T> {
    value: T,
}

interface Labeled {
    function label(self) -> string throws never;
}

class Tag {
    name: string,

    implements Labeled {
        function label(self) -> string throws never {
            self.name
        }
    }
}

function item_class() -> reflect.Class {
    reflect.Type.of<Item>().as_class() ?? throw "Item is a class"
}

function mood_enum() -> reflect.Enum {
    reflect.Type.of<Mood>().as_enum() ?? throw "Mood is an enum"
}

function rendered(view: reflect.TypeView) -> string throws never {
    view.to_string()
}

function described(view: reflect.TypeView) -> string throws reflect.errors.CompilationError {
    let element = view.array().element_type();
    let kind = match (view.kind()) {
        reflect.Class => "class",
        reflect.Enum => "enum",
        _ => "other",
    };
    `${kind} ${element.to_string()} ${view.optional().to_string()} ${view.to_baml()}`
}

test "a class kind and an array of it are field types" {
    let item = item_class();
    let tags = reflect.Class.new(
        "Tags",
        {
            "items": item.array(),
            "first": item.meta(description = "the first item"),
            "mood": mood_enum().optional(),
        },
    );
    let fields = tags.fields();
    assert.equal(fields[0].type, reflect.Type.of<Item[]>());
    assert.equal(fields[1].type, reflect.Type.of<Item>());
    assert.equal(fields[1].meta.description, "the first item");
    assert.equal(fields[2].type, reflect.Type.of<Mood?>())
}

test "a union of kinds is written inline" {
    let union = reflect.Union.new([item_class(), mood_enum(), reflect.Type.of<int>()]);
    assert.equal(union.as_type(), reflect.Type.of<Item | Mood | int>())
}

test "a map takes kinds for its key and value" {
    let string_kind = reflect.Type.of<string>().as_primitive() ?? throw "string is a primitive";
    let by_name = reflect.Map.new(string_kind, item_class());
    assert.equal(by_name.as_type(), reflect.Type.of<map<string, Item>>());
    assert.equal(
        reflect.Map.new(reflect.Type.of<string>(), mood_enum()).as_type(),
        reflect.Type.of<map<string, Mood>>(),
    )
}

test "a builder field and a pending union take kinds" {
    let node = reflect.Class.builder("Node");
    node.field("items", item_class().array());
    node.field("next", node.type().union([mood_enum()]));
    let built = node.build();
    let fields = built.fields();
    assert.equal(fields[0].type, reflect.Type.of<Item[]>());
    assert.equal((fields[1].type ?? throw "typed").to_string(), "Node | kind_views_as_types.Mood")
}

test "type operations run on a class kind" {
    let item = item_class();
    let note = reflect.Type.of<Note>().as_class() ?? throw "Note is a class";
    assert.equal(item.array().as_type(), reflect.Type.of<Item[]>());
    assert.equal(item.optional().as_type(), reflect.Type.of<Item?>());
    assert.equal(note.to_baml(), "class kind_views_as_types.Note {\n  body string\n}");
    assert.is_true(item.kind() is reflect.Class)
}

test "implements and implemented_by take kinds on both sides" {
    let tag = reflect.Type.of<Tag>().as_class() ?? throw "Tag is a class";
    let labeled = reflect.Type.of<Labeled>().as_interface() ?? throw "Labeled is an interface";
    assert.is_true(tag.implements(labeled));
    assert.is_true(labeled.implemented_by(tag));
    assert.is_true(reflect.Type.of<Tag>().implements(labeled));
    assert.is_true(labeled.implemented_by(reflect.Type.of<Tag>()));
    assert.equal(item_class().implements(labeled), false)
}

test "class and enum kinds read their metadata" {
    let meta = item_class().metadata();
    assert.equal(meta.alias, "Entry");
    assert.equal(meta.description, "One item");
    assert.equal(meta.docstring, "An item of a tag list.");
    assert.equal(mood_enum().metadata().skip, false)
}

test "class and enum kinds are named like their type" {
    let item = item_class();
    let mood = mood_enum();
    let boxed = reflect.Type.of<Box<int>>().as_class() ?? throw "Box<int> is a class";
    let runtime = reflect.Class.new("Runtime", { "x": reflect.Type.of<int>() });
    let color = reflect.Enum.new("Color", ["RED"]);

    assert.equal(item.name(), "kind_views_as_types.Item");
    assert.equal(item.to_string(), "kind_views_as_types.Item");
    assert.equal(mood.name(), "kind_views_as_types.Mood");
    assert.equal(mood.to_string(), "kind_views_as_types.Mood");
    assert.equal(boxed.name(), "kind_views_as_types.Box<int>");
    assert.equal(runtime.name(), "Runtime");
    assert.equal(runtime.to_string(), "Runtime");
    assert.equal(color.name(), "Color");
    assert.equal(string.from(runtime), "Runtime");
    assert.equal(rendered(runtime), "Runtime");
    assert.equal(rendered(runtime.as_type()), "Runtime");
    assert.equal(reflect.Union.new([runtime, color]).to_string(), "Runtime | Color")
}

test "a class value and its class kind share a name" {
    let value: reflect.AnyClass = Item { label: "x" } else {
        throw "Item is a class";
    };
    assert.equal(value.name(), (value.type().as_class() ?? throw "a class").name());
    assert.equal(value.name(), item_class().name())
}

test "a TypeView parameter takes reflect.Type and every kind" {
    let expected = "class kind_views_as_types.Note kind_views_as_types.Note | null class kind_views_as_types.Note {\n  body string\n}";
    assert.equal(described(reflect.Type.of<Note>()), expected);
    assert.equal(described(reflect.Type.of<Note>().as_class() ?? throw "Note is a class"), expected);
    assert.equal(
        described(mood_enum()),
        "enum kind_views_as_types.Mood kind_views_as_types.Mood | null enum kind_views_as_types.Mood {\n  Happy\n  Sad\n}",
    )
}
