// `baml.json` beside typed handlers and tasks: each error it throws caught
// by its class and its fields read, selectors that fail as the standard
// library's own handlers decide, and values that cross a `spawn`.

class JsonThrown {
  name: string
  counts: map<string, int>
  ids: int[]
  points: JsonPoint[]
  shades: map<string, JsonShade>
  maybe: float?
}

function json_caught_parse() -> string {
  let text = baml.json.stringify(baml.json.parse(rep("[", 1) + "1,")) catch (e) {
    baml.json.ParseError => `parse|${e.message}`,
  };
  text
}
function json_caught_decode() -> string {
  let x = baml.json.decode<JsonPoint[]>(rep("[", 1) + "{\"x\": 1, \"y\": \"two\"}]")[0].x catch (e) {
    baml.json.DecodeError => {
      e.message.length() * 100 + e.path.length()
    },
  };
  `${x}`
}
function json_caught_decode_fields() -> string {
  let x = `${baml.json.to<JsonPoint>(baml.json.parse(json_doc(1))).x}` catch (e) {
    baml.json.ParseError => "parse",
    baml.json.DecodeError => `decode|${e.message}|${e.path}`,
  };
  x
}
function json_caught_serialization() -> string {
  let text = baml.json.stringify({ "m": [json_int_keys(2)] }) catch (e) {
    baml.json.SerializationError => `${e.message}|${e.path}|${e.reason}`,
  };
  text
}
function json_caught_path() -> string {
  let x = `${baml.json.path<int>(baml.json.parse(json_doc(2)), "[0].z")}` catch (e) {
    baml.json.PathError => `${e.message}|${e.selector}`,
  };
  x
}
function json_caught_not_a_panic() -> string {
  let x = `${baml.json.decode<int>(rep("x", 2))}` catch (e) {
    baml.json.DecodeError => "decode",
    _ => "other",
  };
  x
}

// A leaf that does not decode is a `PathError`, and `path_or` takes every
// `PathError` for its default.
function json_path_leaf() -> int { baml.json.path<int>(baml.json.parse(json_doc(2)), "[1]") }
function json_path_leaf_caught() -> string {
  let x = `${baml.json.path<JsonPoint>(baml.json.parse(json_doc(2)), "[1].y").x}` catch (e) {
    baml.json.DecodeError => "decode",
    baml.json.PathError => `path|${e.message}|${e.selector}`,
  };
  x
}
function json_path_or() -> string {
  let doc = baml.json.parse(json_doc(3));
  let found = baml.json.path_or<int>(doc, "[2].y", -1);
  let missing = baml.json.path_or<int>(doc, "[2].z", -2);
  let misfit = baml.json.path_or<int>(doc, "[2]", -3);
  let malformed = baml.json.path_or<string>(doc, "[2", "none");
  let point = baml.json.path_or<JsonPoint>(doc, "[9]", JsonPoint { x: 7, y: 8 });
  `${found} ${missing} ${misfit} ${malformed} ${point.x}`
}

// Across tasks.
function json_task_decode() -> string {
  let f = spawn { baml.json.decode<JsonPoint[]>(json_doc(4)) };
  let g = spawn { baml.json.parse(json_doc(2)) };
  let points = await f;
  let doc = await g;
  `${points.length()} ${points[3].y} ${baml.json.stringify(doc)}`
}
function json_task_decode_error() -> string {
  let f = spawn { baml.json.decode<JsonPoint>(json_doc(1)).x };
  let x = `${await f}` catch (e) {
    baml.json.DecodeError => `${e.message}|${e.path}`,
  };
  x
}
function json_task_parse_error_uncaught() -> int {
  let f = spawn { baml.json.decode<JsonPoint>(rep("{", 2)).x };
  await f
}
function json_task_values_kept() -> string {
  let doc = baml.json.parse(json_doc(5));
  let everything = baml.json.decode<JsonEverything>(baml.json.stringify(json_everything(3)));
  let f = spawn { task_churn(40) + baml.json.stringify(doc).length() };
  let g = spawn { baml.json.decode<JsonPoint[]>(json_doc(6)).length() + everything.points.length() };
  let churned = task_churn(25);
  let n = (await f) + (await g);
  `${n} ${churned} ${everything.name} ${everything.named["origin"].x} ${baml.json.stringify(baml.json.field(doc, "none"))} ${baml.json.path<int>(doc, "[4].y")}`
}

// An instance thrown and not caught is written with its arrays and maps.
function json_thrown_holder() -> int {
  let counts: map<string, int> = {};
  let shades: map<string, JsonShade> = {};
  let ids: int[] = [];
  for (let i = 0; i < 3; i += 1) {
    counts[rep("k", i + 1)] = i;
    shades[rep("s", i + 1)] = JsonShade.Dark;
    ids.push(i * i);
  };
  let points = baml.json.decode<JsonPoint[]>(json_doc(2));
  if ids.length() == 3 { throw JsonThrown { name: "t", counts: counts, ids: ids, points: points, shades: shades, maybe: halves(3) } }
  0
}

// What only a static type could tell apart, held where nothing says it.
function json_hide(value: unknown, n: int) -> unknown {
  let held: unknown[] = [];
  for (let i = 0; i < n; i += 1) { held.push(value); };
  held[n - 1]
}
function json_unknown_scalar_array() -> string {
  let ids: int[] = [];
  for (let i = 0; i < 3; i += 1) { ids.push(i); };
  baml.json.stringify(json_hide(ids, 2))
}
function json_unknown_scalar_map() -> string {
  let counts: map<string, int> = {};
  for (let i = 0; i < 2; i += 1) { counts[rep("k", i + 1)] = i; };
  baml.json.stringify(json_hide(counts, 2))
}
// A decode follows a value 128 levels down and no further; the
// interpreter's limit is on the text it parses only.
function json_to_deep() -> string {
  let j: json = null;
  for (let i = 0; i < 140; i += 1) { j = { "value": i, "next": j }; };
  let value = `${baml.json.to<JsonNode>(j).value}` catch (e) {
    baml.json.DecodeError => e.message,
  };
  value
}
function json_to_deep_within() -> int {
  let j: json = null;
  for (let i = 0; i < 120; i += 1) { j = { "value": i, "next": j }; };
  baml.json.to<JsonNode>(j).value
}
function json_decode_aliased_without_skipped() -> string {
  let named = baml.json.decode<JsonNamed>(rep("{", 1) + "\"firstName\": \"Bea\", \"kept_as\": \"k\", \"shade\": \"light\"}", naming = baml.json.Naming.Aliased);
  `${named.first_name} ${named.secret}`
}

// The natives at their depth limits, called by the deepest call that had
// room to be entered: they run in what compiled code leaves the runtime.
function json_dive(depth: int, good: string, bad: string) -> int {
  let deeper = json_dive(depth + 1, good, bad) catch (e) {
    baml.panics.StackOverflow => -1,
  };
  if deeper == -1 {
    let node = baml.json.decode<JsonNode>(good);
    let text = baml.json.stringify(baml.json.from(node));
    let message = baml.json.stringify(baml.json.parse(bad)) catch (e) {
      baml.json.ParseError => e.message,
    };
    let leaf = baml.json.path_or<int>(baml.json.parse(good), ".next.next.value", -5);
    return text.length() * 1000 + message.length() + leaf;
  }
  deeper
}
function json_deep_node() -> string {
  rep("{\"value\": 1, \"next\": ", 126) + "null" + rep("}", 126)
}
function json_at_stack_limit() -> int {
  json_dive(0, json_deep_node(), rep("[", 126) + "x")
}
function json_at_task_stack_limit() -> int {
  let good = json_deep_node();
  let f = spawn { json_dive(0, good, rep("[", 126) + "x") };
  await f
}
