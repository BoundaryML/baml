// Round-trip every BEP-030 Ty variant through codegen.
//   - `unknown` → typing.Any
//   - function type `(int) -> string` → typing.Callable[[int], str]
//   - recursive `type` alias
//   - bare string literal in field position
//   - optional nested in list (and self-ref optional)
//   - union of primitives
class MyClass {
  id int
}

class CoverAll {
  unknown_field unknown
  callable_field (int) -> string
  alias_field RecursiveAlias
  literal_field "Hello"
  optional_nested (MyClass?)[]
  union_field int | string
  self_ref CoverAll?
}

type RecursiveAlias = int | RecursiveAlias[]
