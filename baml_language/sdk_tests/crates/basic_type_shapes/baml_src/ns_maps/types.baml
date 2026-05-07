// Map-typed fields. Class-only: the original fixture's non-LLM
// functions (map_string_int / nested_map / map_of_arrays) are
// unrepresentable in user BAML (no body, or `void` return), so they're
// dropped here. Function emission coverage lives in
// `python_example_09a`.
class MapContainer {
  simple map<string, int>
  nested map<string, map<string, string>>
  array_val map<string, string[]>
  union_val map<string, string | int>
}
