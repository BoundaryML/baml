// Literal-typed fields. Each field's type is a bare literal value,
// producing `typing.Literal[...]` in Python via 09b. Covers string,
// int, and bool literal types.
class Literals {
  priority_1 "1"
  priority_2 "2"
  priority_3 "3"
  status_draft "draft"
  status_published "published"
  count 42
  enabled true
  disabled false
}
