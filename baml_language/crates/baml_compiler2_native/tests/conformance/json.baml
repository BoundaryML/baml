// `baml.json`: text read into `json` values and written from them, values
// of declared types written and read by their descriptions, and what each
// refuses. Documents are built at run time, on the heap.

enum JsonColor { Red Green Blue }
enum JsonShade {
  Light @alias("light")
  Dark @alias("dark")
  Hidden @skip
}
class JsonPoint {
  x: int
  y: int
}
class JsonEverything {
  name: string
  count: int
  ratio: float
  on: bool
  maybe_int: int?
  maybe_float: float?
  maybe_bool: bool?
  maybe_text: string?
  point: JsonPoint
  maybe_point: JsonPoint?
  color: JsonColor
  points: JsonPoint[]
  numbers: int[]
  floats: float[]
  flags: bool[]
  colors: JsonColor[]
  grid: int[][]
  counts: map<string, int>
  named: map<string, JsonPoint>
  either: int | string
  extra: json
}
class JsonNamed {
  first_name: string @alias("firstName")
  secret: int @skip
  kept: string @json.rename("kept_as")
  shade: JsonShade
}
class JsonSparse {
  a: int?
  b: string?
  c: int
  @@json.omit_nulls
}
class JsonNode {
  value: int
  next: JsonNode?
}
class JsonCircle {
  kind: "circle"
  radius: float
}
class JsonSquare {
  kind: "square"
  side: float
  label: string?
}
class JsonHolder {
  shape: JsonCircle | JsonSquare
  level: "low" | "high"
  id: int | string
  tint: string | JsonColor
  amount: float | int
}

function json_doc(n: int) -> string {
  let parts: string[] = [];
  for (let i = 0; i < n; i += 1) { parts.push(`{"x": ${i}, "y": ${i * i}}`); };
  "[" + parts.join(", ") + "]"
}
function json_squares(n: int) -> map<string, int> {
  let m: map<string, int> = {};
  for (let i = 0; i < n; i += 1) { m[`k${i}`] = i * i; };
  m
}
function json_int_keys(n: int) -> map<int, int> {
  let m: map<int, int> = {};
  for (let i = 0; i < n; i += 1) { m[i] = i; };
  m
}
function json_everything(n: int) -> JsonEverything {
  JsonEverything {
    name: rep("n", n),
    count: -7,
    ratio: halves(3),
    on: true,
    maybe_int: 4,
    maybe_float: null,
    maybe_bool: false,
    maybe_text: null,
    point: JsonPoint { x: 1, y: 2 },
    maybe_point: null,
    color: JsonColor.Green,
    points: [JsonPoint { x: 3, y: 4 }, JsonPoint { x: 5, y: 6 }],
    numbers: [1, 2, 3],
    floats: [0.5, 1.0, 1e21],
    flags: [true, false],
    colors: [JsonColor.Blue, JsonColor.Red],
    grid: [[1], [], [2, 3]],
    counts: { "b": 2, "a": 1 },
    named: { "origin": JsonPoint { x: 0, y: 0 } },
    either: rep("e", 2),
    extra: { "k": [1, 2.5, "s", true, null, { "z": {} }] },
  }
}

// Text to `json` and back.
function json_parse_scalars() -> string {
  baml.json.stringify(baml.json.parse(rep(" ", 2) + "[1, -2, 0, 1.5, -0.0, 1e3, 1E-7, 1e21, 2.5e-3, 123456789012, true, false, null]\n"))
}
function json_parse_escapes() -> string {
  baml.json.stringify(baml.json.parse(rep("[", 1) + "\"q\\\" b\\\\ s\\/ \\b\\f\\n\\r\\t \\u00e9 \\ud83d\\ude00 \\u0001 \\u001f é 😀\"]"))
}
function json_parse_escaped_text() -> int {
  let j = baml.json.parse(rep("[", 1) + "\"\\u00e9\\ud83d\\ude00a\"]");
  baml.json.to<string[]>(j)[0].length()
}
function json_stringify_text() -> string {
  baml.json.stringify(rep("é😀\"\\\n\t/", 2))
}
function json_floats() -> string {
  baml.json.stringify([halves(1) / 5.0 + 0.2, tens(21), 1.0 / tens(7), halves(2), tens(2), tens(15), tens(16), tens(17), 123456789.125, 0.0 - halves(0), 5e-324, 1.7976931348623157e308, 0.30000000000000004])
}
function json_ints() -> string {
  let j = baml.json.parse(rep("[", 1) + "9007199254740993, -4611686018427387904, 4611686018427387903, 0, -1]");
  baml.json.stringify(j)
}
// native throws: baml.json.ParseError {message: "integral JSON number does not fit the native 64-bit int"}
function json_parse_past_int() -> string {
  baml.json.stringify(baml.json.parse(rep("9", 19) + "0"))
}
function json_parse_float_past_range() -> string {
  baml.json.stringify(baml.json.parse(rep("[", 1) + "1,\n 1e400]"))
}
function json_empties() -> string {
  baml.json.stringify(baml.json.parse(rep("[", 1) + "[], {}, [[]], {\"a\": {}}, \"\"]"), indent = 2)
}
function json_depth_within() -> int {
  baml.json.stringify(baml.json.parse(rep("[", 127) + rep("]", 127))).length()
}
function json_depth_past() -> int {
  baml.json.stringify(baml.json.parse(rep("[", 128) + rep("]", 128))).length()
}
function json_key_order() -> string {
  baml.json.stringify(baml.json.parse(rep("{", 1) + "\"z\": 1, \"a\": 2, \"m\": {\"y\": 1, \"b\": 2}}"))
}
function json_duplicate_keys() -> string {
  baml.json.stringify(baml.json.parse(rep("{", 1) + "\"a\": 1, \"b\": 2, \"a\": 3, \"c\": 4, \"b\": [5]}"))
}
function json_many_keys() -> string {
  let parts: string[] = [];
  for (let i = 0; i < 40; i += 1) { parts.push(`"k${i % 37}": ${i}`); };
  baml.json.stringify(baml.json.parse("{" + parts.join(",") + "}"))
}
function json_malformed_empty() -> string { baml.json.stringify(baml.json.parse(rep(" ", 2))) }
function json_malformed_open() -> string { baml.json.stringify(baml.json.parse(rep("{", 1))) }
function json_malformed_comma() -> string { baml.json.stringify(baml.json.parse(rep("[", 1) + "1,]")) }
function json_malformed_colon() -> string { baml.json.stringify(baml.json.parse(rep("{", 1) + "\"a\" 1}")) }
function json_malformed_word() -> string { baml.json.stringify(baml.json.parse(rep("n", 1) + "ul")) }
function json_malformed_zero() -> string { baml.json.stringify(baml.json.parse(rep("0", 1) + "1")) }
function json_malformed_fraction() -> string { baml.json.stringify(baml.json.parse(rep("1", 1) + ".")) }
function json_malformed_escape() -> string { baml.json.stringify(baml.json.parse(rep("\"", 1) + "\\x\"")) }
function json_malformed_surrogate() -> string { baml.json.stringify(baml.json.parse(rep("\"", 1) + "\\ud83d\"")) }
function json_malformed_string() -> string { baml.json.stringify(baml.json.parse(rep("\"", 1) + "abc")) }
function json_malformed_control() -> string { baml.json.stringify(baml.json.parse(rep("\"", 1) + "a\tb\"")) }
function json_malformed_trailing() -> string { baml.json.stringify(baml.json.parse(rep("[", 1) + "1]\n x")) }
function json_malformed_key() -> string { baml.json.stringify(baml.json.parse(rep("{", 1) + "1: 2}")) }

// Layout.
function json_indent_spaces() -> string { baml.json.stringify(json_everything(3), indent = 2) }
function json_indent_tab() -> string { baml.json.stringify(baml.json.parse(json_doc(2)), indent = "\t") }
function json_indent_none() -> string { baml.json.stringify(baml.json.parse(json_doc(2)), indent = "") }
function json_indent_too_wide() -> string { baml.json.stringify(json_doc(1), indent = 11) }
function json_indent_negative() -> string { baml.json.stringify(json_doc(1), indent = 0 - 1) }
function json_indent_text() -> string { baml.json.stringify(json_doc(1), indent = "ab") }
function json_indent_long() -> string { baml.json.stringify(json_doc(1), indent = rep(" ", 11)) }

// Values of declared types, written.
function json_stringify_class() -> string { baml.json.stringify(json_everything(3)) }
function json_stringify_omit_nulls() -> string { baml.json.stringify(json_everything(3), omit_nulls = true) }
function json_stringify_scalars() -> string {
  let n: int? = null;
  baml.json.stringify(41 + 1) + baml.json.stringify(halves(1)) + baml.json.stringify(true) + baml.json.stringify(null) + baml.json.stringify(n) + baml.json.stringify(JsonColor.Blue)
}
function json_stringify_lists() -> string {
  let empty: int[] = [];
  let maybe: string?[] = ["a", null];
  baml.json.stringify([1, 2]) + baml.json.stringify(empty) + baml.json.stringify([[halves(1)], []]) + baml.json.stringify(maybe) + baml.json.stringify([JsonColor.Red])
}
function json_stringify_maps() -> string {
  let empty: map<string, int> = {};
  let colors: map<string, JsonColor> = { "c": JsonColor.Green };
  let lists: map<string, float[]> = { "l": [halves(1)] };
  baml.json.stringify(json_squares(12)) + baml.json.stringify(empty) + baml.json.stringify(colors) + baml.json.stringify(lists)
}
function json_stringify_unknown() -> string {
  let held: unknown = json_everything(3).point;
  let text: unknown = rep("u", 2);
  let number: unknown = 5;
  baml.json.stringify(held) + baml.json.stringify(text) + baml.json.stringify(number)
}
function json_stringify_names() -> string {
  let named = JsonNamed { first_name: "Ada", secret: 9, kept: "k", shade: JsonShade.Dark };
  baml.json.stringify(named) + baml.json.stringify(named, naming = baml.json.Naming.Aliased)
}
function json_stringify_skipped_variant() -> string {
  baml.json.stringify(JsonNamed { first_name: "Ada", secret: 9, kept: "k", shade: JsonShade.Hidden }, naming = baml.json.Naming.Aliased)
}
function json_stringify_sparse() -> string {
  baml.json.stringify([JsonSparse { a: null, b: null, c: 1 }, JsonSparse { a: 2, b: "b", c: 3 }])
}
function json_stringify_unions() -> string {
  let holder = JsonHolder { shape: JsonSquare { kind: "square", side: 2.0, label: null }, level: "high", id: 7, tint: JsonColor.Red, amount: 3 };
  let other = JsonHolder { shape: JsonCircle { kind: "circle", radius: 0.5 }, level: "low", id: "seven", tint: "teal", amount: 2.5 };
  baml.json.stringify([holder, other])
}
function json_stringify_linked() -> string {
  let head = JsonNode { value: 0, next: null };
  for (let i = 1; i < 300; i += 1) { head = JsonNode { value: i, next: head }; };
  let text = baml.json.stringify(head);
  `${text.length()}`
}
function json_stringify_cycle() -> string {
  let a = JsonNode { value: 1, next: null };
  let b = JsonNode { value: 2, next: a };
  a.next = b;
  baml.json.stringify([a])
}
function json_stringify_not_finite() -> string {
  baml.json.stringify({ "v": [1.0, tens(308) * tens(308)] })
}
function json_stringify_int_keys() -> string { baml.json.stringify({ "m": json_int_keys(2) }) }
function json_from() -> string {
  let j = baml.json.from(json_everything(3));
  baml.json.stringify(baml.json.field(j, "points")) + baml.json.stringify(baml.json.field(j, "missing")) + baml.json.stringify(baml.json.field(baml.json.field(j, "name"), "x"))
}
function json_from_omit_nulls() -> string {
  baml.json.stringify(baml.json.from(json_everything(3), omit_nulls = true))
}

// Selectors.
function json_path_values() -> string {
  let doc = baml.json.parse(rep("{", 1) + "\"users\": [{\"name\": \"Ada\", \"age\": 36, \"tags\": [\"x\", \"y\"], \"a.b\": null}]}");
  let age = baml.json.path<int>(doc, ".users[0].age");
  let name = baml.json.path<string>(doc, "[\"users\"][0]['name']");
  let tags = baml.json.path<string[]>(doc, ".users[0].tags");
  let none = baml.json.path<int?>(doc, ".users[0][\"a.b\"]");
  let whole = baml.json.path<json>(doc, ".");
  `${age} ${name} ${tags.length()} ${none ?? -1} ${baml.json.stringify(whole).length()}`
}
function json_path_missing() -> int { baml.json.path<int>(baml.json.parse(json_doc(2)), "[0].z") }
function json_path_non_object() -> int { baml.json.path<int>(baml.json.parse(json_doc(2)), ".x") }
function json_path_non_array() -> int { baml.json.path<int>(baml.json.parse(json_doc(2)), "[0][1]") }
function json_path_bounds() -> int { baml.json.path<int>(baml.json.parse(json_doc(2)), "[2].x") }
function json_path_negative() -> int { baml.json.path<int>(baml.json.parse(json_doc(2)), "[-1].x") }
function json_path_token() -> int { baml.json.path<int>(baml.json.parse(json_doc(2)), "[x]") }
function json_path_start() -> int { baml.json.path<int>(baml.json.parse(json_doc(2)), "x") }
function json_path_unterminated() -> int { baml.json.path<int>(baml.json.parse(json_doc(2)), "[0") }
function json_path_quote() -> int { baml.json.path<int>(baml.json.parse(json_doc(2)), "[\"x]") }

// Text and `json` values read into declared types.
function json_decode_class() -> string {
  let points = baml.json.decode<JsonPoint[]>(json_doc(3));
  let out = "";
  for (let p in points) { out = `${out}${p.x},${p.y};`; };
  out
}
function json_decode_everything() -> string {
  let text = baml.json.stringify(json_everything(3));
  let back = baml.json.decode<JsonEverything>(text);
  let grid = back.grid[2];
  `${back.name} ${back.count} ${back.ratio} ${back.on} ${back.maybe_int ?? -1} ${back.maybe_float ?? -1.0} ${back.maybe_bool ?? true} ${back.maybe_text ?? "none"} ${back.point.y} ${back.points[1].x} ${back.numbers.length()} ${back.floats[2]} ${back.flags[1]} ${back.colors[0]} ${grid[1]} ${back.counts["a"]} ${back.named["origin"].x} ${baml.json.stringify(back.extra)} ${baml.json.stringify(back) == text}`
}
function json_decode_absent_and_extra() -> string {
  let node = baml.json.decode<JsonNode>(rep("{", 1) + "\"ignored\": [1, {\"deep\": true}], \"value\": 3}");
  let sparse = baml.json.decode<JsonSparse>(rep("{", 1) + "\"c\": 1, \"b\": null}");
  `${node.value} ${node.next == null} ${sparse.a ?? -1} ${sparse.b ?? "none"} ${sparse.c}`
}
function json_decode_scalars() -> string {
  let a = baml.json.decode<int>(rep("4", 1) + "2");
  let b = baml.json.decode<float>(rep("4", 1) + "2");
  let c = baml.json.decode<float>(rep("4", 1) + ".5");
  let d = baml.json.decode<bool>(rep("t", 1) + "rue");
  let e = baml.json.decode<string>(rep("\"", 1) + "s\"");
  let f = baml.json.decode<int?>(rep("n", 1) + "ull");
  let g = baml.json.decode<int?>(rep("5", 1));
  let h = baml.json.decode<JsonColor>(rep("\"", 1) + "Blue\"");
  let i = baml.json.decode<float?>(rep("n", 1) + "ull");
  `${a} ${b} ${c} ${d} ${e} ${f ?? -1} ${g ?? -1} ${h} ${i ?? -1.0}`
}
function json_decode_containers() -> string {
  let lists = baml.json.decode<float[][]>(rep("[", 1) + "[1, 2.5], []]");
  let counts = baml.json.decode<map<string, int>>(rep("{", 1) + "\"b\": 2, \"a\": 1, \"b\": 3}");
  let named = baml.json.decode<map<string, JsonPoint?>>(rep("{", 1) + "\"p\": {\"x\": 1, \"y\": 2}, \"q\": null}");
  let colors = baml.json.decode<JsonColor[]>(rep("[", 1) + "\"Red\", \"Green\"]");
  `${lists[0][1]} ${lists[1].length()} ${baml.json.stringify(counts)} ${named.length()} ${colors[1]}`
}
function json_decode_unions() -> string {
  let text = rep("[", 1) + "{\"shape\": {\"kind\": \"square\", \"side\": 2}, \"level\": \"high\", \"id\": 7, \"tint\": \"Red\", \"amount\": 3}, {\"shape\": {\"kind\": \"circle\", \"radius\": 0.5}, \"level\": \"low\", \"id\": \"seven\", \"tint\": \"teal\", \"amount\": 2.5}]";
  baml.json.stringify(baml.json.decode<JsonHolder[]>(text))
}
function json_decode_union_members() -> string {
  let a = baml.json.decode<float | int>(rep("1", 1));
  let b = baml.json.decode<float | int>(rep("1", 1) + ".5");
  let c = baml.json.decode<int | string | null>(rep("n", 1) + "ull");
  let d = baml.json.decode<JsonPoint | map<string, int>>(rep("{", 1) + "\"x\": 1, \"y\": 2, \"z\": 3}");
  let e = baml.json.decode<JsonPoint | JsonNode>(rep("{", 1) + "\"value\": 1, \"x\": 1, \"y\": 2}");
  baml.json.stringify(a) + baml.json.stringify(b) + baml.json.stringify(c) + baml.json.stringify(d) + baml.json.stringify(e)
}
function json_decode_names() -> string {
  let declared = baml.json.decode<JsonNamed>(rep("{", 1) + "\"first_name\": \"Ada\", \"secret\": 9, \"kept_as\": \"k\", \"shade\": \"Dark\"}");
  let aliased = baml.json.decode<JsonNamed>(rep("{", 1) + "\"firstName\": \"Bea\", \"secret\": 9, \"kept_as\": \"k\", \"shade\": \"light\"}", naming = baml.json.Naming.Aliased);
  `${declared.first_name} ${declared.secret} ${declared.kept} ${declared.shade} ${aliased.first_name} ${aliased.secret} ${aliased.kept} ${aliased.shade}`
}
function json_decode_json() -> string {
  let j = baml.json.decode<json>(json_doc(2));
  let m = baml.json.decode<map<string, json>>(rep("{", 1) + "\"a\": [1, {\"b\": null}]}");
  baml.json.stringify(j) + baml.json.stringify(m)
}
function json_to_from_built() -> string {
  let built: json = { "x": 4, "y": 5, "more": [1.5, "s"] };
  let point = baml.json.to<JsonPoint>(built);
  let node = baml.json.to<JsonNode>(baml.json.from(JsonNode { value: 1, next: JsonNode { value: 2, next: null } }));
  let next = node.next;
  `${point.x} ${point.y} ${node.value} ${next == null}`
}
function json_decode_missing() -> int { baml.json.decode<JsonPoint>(rep("{", 1) + "\"x\": 1}").x }
function json_decode_wrong_kind() -> int { baml.json.decode<JsonPoint[]>(rep("[", 1) + "{\"x\": 1, \"y\": 2}, {\"x\": 1, \"y\": \"2\"}]").length() }
function json_decode_not_object() -> int { baml.json.decode<JsonNode>(rep("{", 1) + "\"value\": 1, \"next\": [1]}").value }
function json_decode_fraction_as_int() -> int { baml.json.decode<int>(rep("1", 1) + ".0") }
function json_decode_text_as_int() -> int { baml.json.decode<int>(rep("\"", 1) + "5\"") }
function json_decode_int_as_text() -> string { baml.json.decode<string>(rep("5", 1)) }
function json_decode_int_as_bool() -> bool { baml.json.decode<bool>(rep("1", 1)) }
function json_decode_object_as_list() -> int { baml.json.decode<int[]>(rep("{", 1) + "}").length() }
function json_decode_list_as_map() -> int { baml.json.decode<map<string, int>>(rep("[", 1) + "]").length() }
function json_decode_map_value() -> int { baml.json.decode<map<string, int>>(rep("{", 1) + "\"a\": 1, \"b c\": null}").length() }
function json_decode_unknown_variant() -> string { `${baml.json.decode<JsonColor>(rep("\"", 1) + "Purple\"")}` }
function json_decode_variant_kind() -> string { `${baml.json.decode<JsonColor>(rep("1", 1))}` }
function json_decode_skipped_variant() -> string { `${baml.json.decode<JsonShade>(rep("\"", 1) + "Hidden\"", naming = baml.json.Naming.Aliased)}` }
function json_decode_literal() -> string { baml.json.stringify(baml.json.decode<JsonCircle>(rep("{", 1) + "\"kind\": \"square\", \"radius\": 1}")) }
function json_decode_no_member() -> string { baml.json.stringify(baml.json.decode<int | string>(rep("[", 1) + "]")) }
function json_decode_no_class_member() -> string { baml.json.stringify(baml.json.decode<JsonHolder>(rep("{", 1) + "\"shape\": {\"kind\": \"cube\"}, \"level\": \"low\", \"id\": 1, \"tint\": \"x\", \"amount\": 1}")) }
function json_decode_syntax() -> int { baml.json.decode<JsonPoint>(rep("{", 1) + "\"x\": 1,}").x }
// native prints: 4611686018427387904
function json_decode_past_small_int() -> int { baml.json.decode<int>(rep("4", 1) + "611686018427387904") }
