// `baml.yaml.parse`: one document, read as a `json`.

function yaml_scalars() -> string {
  let found = baml.yaml.parse(rep("a: 1\nb: 2.5\nc: true\nd: null\ne: text\nf: \"quoted: yes\"\n", 1));
  baml.json.stringify(found)
}
function yaml_nested() -> string {
  let found = baml.yaml.parse(rep("list:\n  - 1\n  - [2, 3]\n  - {x: 1, y: [a, b]}\nmap:\n  inner:\n    deep: -4\n", 1));
  baml.json.stringify(found)
}
// The keys of a mapping stay in the order written.
function yaml_key_order() -> string {
  baml.json.stringify(baml.yaml.parse(rep("zebra: 1\napple: 2\nmango: 3\n", 1)))
}
function yaml_strings() -> string {
  baml.json.stringify(baml.yaml.parse(rep("- \"tab\\tquote\\\" back\\\\slash\"\n- 'é → ü'\n- |\n  two\n  lines\n- \"\\u0001\"\n", 1)))
}
function yaml_numbers() -> string {
  baml.json.stringify(baml.yaml.parse(rep("[0, -7, 4611686018427387903, 1.0, 1e3, -0.5, 0x10, 1_000]", 1)))
}
function yaml_empty_stream() -> bool {
  baml.yaml.parse(rep("", 1)) == null
}
function yaml_null_document() -> bool {
  baml.yaml.parse(rep("~", 1)) == null
}
function yaml_decode() -> string {
  let found = baml.yaml.decode<map<string, int[]>>(rep("a: [1, 2]\nb: []\n", 1));
  `${found["a"].length()} ${found["b"].length()} ${found["a"][1]}`
}
function yaml_long() -> int {
  let found = baml.yaml.parse(rep("- {name: item, tags: [x, y, z]}\n", 300));
  baml.json.stringify(found).length()
}
function yaml_deep() -> int {
  baml.json.stringify(baml.yaml.parse(rep("[", 60) + rep("]", 60))).length()
}
function yaml_syntax_error() -> bool {
  baml.yaml.parse(rep("a: [1, 2", 1)) == null
}
function yaml_syntax_error_caught() -> string {
  let found = baml.yaml.parse(rep("a: b: c: [", 1)) catch (e) {
    baml.yaml.ParseError => { return e.message; },
  };
  baml.json.stringify(found)
}
function yaml_two_documents() -> bool {
  baml.yaml.parse(rep("a: 1\n---\nb: 2\n", 1)) == null
}
function yaml_key_not_a_string() -> bool {
  baml.yaml.parse(rep("1: one\n", 1)) == null
}
function yaml_tag() -> bool {
  baml.yaml.parse(rep("a: !custom 1\n", 1)) == null
}
function yaml_infinite() -> bool {
  baml.yaml.parse(rep("a: .inf\n", 1)) == null
}
function yaml_int_too_large() -> bool {
  baml.yaml.parse(rep("a: 18446744073709551615\n", 1)) == null
}
function yaml_int_past_63_bits() -> string {
  baml.json.stringify(baml.yaml.parse(rep("a: 9223372036854775807\n", 1)))
}
