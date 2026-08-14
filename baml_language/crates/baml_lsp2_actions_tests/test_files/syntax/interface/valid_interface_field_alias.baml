// BEP-044: interface fields are linked to class-owned fields with
// `interface_field as class_field`.

interface Named {
  name: string
}

class Person {
  display_name: string

  implements Named {
    name as display_name
  }
}

function read_name(p: Person) -> string {
  return p.as<Named>.name
}

//----
//- diagnostics
// <no-diagnostics-expected>
