// A method call written on an enum variant path.
//
// `EvmpColor.Red.to_string()` reaches the compiler as one flat path
// (`EvmpColor`, `Red`, `to_string`): only name resolution can tell that the
// first two segments are a value and the last is a member read off it. The
// call resolves like the same call on a `let` binding of the variant, for a
// variant of an enum in this namespace or in another one, and a path that
// names a function or a static through its last segment keeps its meaning.

enum EvmpColor {
    Red
    Green
}

class EvmpSwatch {
    color EvmpColor

    function first() -> EvmpSwatch {
        EvmpSwatch { color: EvmpColor.Red }
    }
}

function evmp_let_to_string() -> string {
    let color = EvmpColor.Red;
    color.to_string()
}

function evmp_path_to_string() -> string {
    EvmpColor.Red.to_string()
}

// A method other than `to_string`: the receiver is the variant, whatever the
// member is.
function evmp_path_to_json() -> string {
    baml.json.to_string(EvmpColor.Green.to_json())
}

// The result is an ordinary value: a further call chains off it, and the call
// is an operand like any other.
function evmp_path_chained() -> int {
    EvmpColor.Green.to_string().length()
}

function evmp_path_operand() -> string {
    "color=" + EvmpColor.Red.to_string() + "," + EvmpColor.Green.to_string()
}

function evmp_path_in_lambda() -> string[] {
    let colors: EvmpColor[] = [EvmpColor.Red, EvmpColor.Green];
    colors.map((color: EvmpColor) -> string throws never {
        color.to_string() + "/" + EvmpColor.Red.to_string()
    })
}

// An enum in another namespace: by a path relative to this namespace, and by
// its full path from the package root.
function evmp_namespaced_relative() -> string {
    palette.Tone.Warm.to_string()
}

function evmp_namespaced_absolute() -> string {
    root.enum_variant_method_paths.palette.Tone.Cool.to_string()
}

function evmp_namespaced_to_json() -> string {
    baml.json.to_string(palette.Tone.Cool.to_json())
}

// ── paths that keep their meaning ──────────────────────────────────────────

// `Type.from_json` is the decode sugar on the written type, and a variant is a
// type too: the last segment is not a member of the variant's value.
function evmp_variant_from_json() -> EvmpColor {
    EvmpColor.Red.from_json(baml.json.from("Red"))
}

// A static method of a class, with and without the namespace.
function evmp_static_method() -> string {
    EvmpSwatch.first().color.to_string()
}

function evmp_namespaced_static_method() -> string {
    root.enum_variant_method_paths.EvmpSwatch.first().color.to_string()
}

// A function in a namespace, and a function of a builtin package.
function evmp_namespaced_function() -> string {
    palette.Tone.Warm.to_string() + ":" + palette.label(palette.Tone.Warm)
}

function evmp_absolute_function() -> string {
    root.enum_variant_method_paths.palette.label(palette.Tone.Cool)
}

function evmp_package_static() -> bool {
    reflect.Package.current().get_enum("enum_variant_method_paths.EvmpColor") != null
}

function evmp_package_function() -> string {
    ai.clients.resolve("openai/gpt-4o-mini").id()
}

test "a_method_call_on_a_variant_path_matches_the_let_form" {
    assert.equal(evmp_let_to_string(), "Red")
    assert.equal(evmp_path_to_string(), "Red")
}

test "a_variant_path_takes_any_method_of_the_value" {
    assert.equal(evmp_path_to_json(), "\"Green\"")
}

test "a_variant_path_call_is_an_ordinary_value" {
    assert.equal(evmp_path_chained(), 5)
    assert.equal(evmp_path_operand(), "color=Red,Green")
    assert.equal(evmp_path_in_lambda(), ["Red/Red", "Green/Red"])
}

test "a_variant_of_a_namespaced_enum_takes_a_method_call" {
    assert.equal(evmp_namespaced_relative(), "Warm")
    assert.equal(evmp_namespaced_absolute(), "Cool")
    assert.equal(evmp_namespaced_to_json(), "\"Cool\"")
}

test "from_json_on_a_variant_path_is_still_the_decode_sugar" {
    assert.equal(evmp_variant_from_json(), EvmpColor.Red)
}

test "function_and_static_paths_keep_their_meaning" {
    assert.equal(evmp_static_method(), "Red")
    assert.equal(evmp_namespaced_static_method(), "Red")
    assert.equal(evmp_namespaced_function(), "Warm:tone Warm")
    assert.equal(evmp_absolute_function(), "tone Cool")
    assert.equal(evmp_package_static(), true)
    assert.equal(evmp_package_function(), "openai/gpt-4o-mini")
}
