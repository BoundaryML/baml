// Union-typed fields and union arms across primitives and classes.
// Class-only: the original fixture's non-LLM functions (union_simple,
// union_complex, union_in_list, union_return) are unrepresentable in
// user BAML — `void` returns and bodyless declarations don't exist
// in the grammar — so they're dropped here. Function emission
// coverage lives in `python_example_09a` / `python_llm_functions`.
class User {
  name string
}

class Company {
  name string
  industry string
}

class Container {
  items (string | int | bool)[]
  matrix (string | int)[][]
  optional_union (string | int)?
}
