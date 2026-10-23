// B-1885: substituting a nullable container element into T? must flatten
// every union/alias before optional access and null coalescing.
class Group {
    text: string,
}

type MaybeGroup = Group?;

type NestedGroup = MaybeGroup | null;

type MaybeCallback = ((left: int, right: int) -> string throws never)?;

type RepeatedCallback = MaybeCallback | ((left: 1 | int, right: int) -> string throws never)?;

type MaybeInts = int[]?;

type NamedCallback = (left: int, right: int) -> int throws never;

type Callbacks = NamedCallback[];

type CallbackMap = map<string, NamedCallback>;

function call_list(fs: Callbacks) -> int {
    fs[0](right = 2, left = 1)
}

function call_map(fs: CallbackMap, key: string) -> int {
    fs.get(key)?.(right = 2, left = 1) ?? -1
}

function map_text(m: map<string, Group?>, key: string) -> string {
    m.get(key)?.text ?? "missing"
}

function array_text(xs: Group?[], index: int) -> string {
    xs.at(index)?.text ?? "missing"
}

function alias_text(g: NestedGroup?) -> string {
    g?.text ?? "missing"
}

function callback_text(f: RepeatedCallback?) -> string {
    f?.(right = 2, left = 1) ?? "missing"
}

function first_int(xs: MaybeInts?) -> int {
    xs?.[0] ?? -1
}

function capture_text(text: string) -> string {
    let m = baml.regex.new("(a)?b").match_once(text);
    if let found: baml.regex.Match = m {
        found.groups.at(1)?.text ?? "missing"
    } else {
        "no match"
    }
}

test "nested_nullable_map_get" {
    let m: map<string, Group?> = { "some": Group { text: "hi" }, "none": null };
    assert.equal(map_text(m, "some"), "hi");
    assert.equal(map_text(m, "none"), "missing");
    assert.equal(map_text(m, "absent"), "missing")
}

test "nested_nullable_array_at" {
    let xs: Group?[] = [null, Group { text: "hi" }];
    assert.equal(array_text(xs, 0), "missing");
    assert.equal(array_text(xs, 1), "hi");
    assert.equal(array_text(xs, 2), "missing")
}

test "nested_nullable_aliases" {
    assert.equal(alias_text(Group { text: "hi" }), "hi");
    assert.equal(alias_text(null), "missing");
    assert.equal(
        callback_text((left, right) -> {
            (left + right).to_string()
        }),
        "3",
    );
    assert.equal(callback_text(null), "missing");
    assert.equal(first_int([7]), 7);
    assert.equal(first_int(null), -1)
}

test "nested_nullable_regex_capture" {
    assert.equal(capture_text("ab"), "a");
    assert.equal(capture_text("b"), "missing");
    assert.equal(capture_text("x"), "no match")
}

test "nested_nullable_callable_containers" {
    let cb: NamedCallback = (left, right) -> {
        left * 10 + right
    };
    assert.equal(call_list([cb]), 12);
    assert.equal(call_map({ "present": cb }, "present"), 12);
    assert.equal(call_map({ "present": cb }, "absent"), -1)
}
