class Person<T> {
  name T

  function greeting(self) -> T {
    return self.name
  }
}

function ReadPerson(person: Person<string>) -> string {
  let name = person.na<[CURSOR]me
  return person.gree<[CURSOR]ting()
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at b_971_class_member_references.baml:10:23
// ```baml
// name: string
// ```
// hover at b_971_class_member_references.baml:11:21
// ```baml
// greeting: (self: Person<string>) -> string throws never
// ```
