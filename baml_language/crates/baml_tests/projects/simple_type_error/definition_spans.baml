function wrong_arity_definition() -> int {
  Foo_arity(1, 2)
}

function Foo_arity(x: int) -> int {
  x
}

function missing_field_access() -> int {
  let person = Person { name: "Ada" };
  person.age
}

class Person {
  name string
}
