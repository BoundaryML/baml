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
//
//- semantic_tokens
// field_alias.baml:1:1 (comment) len=66 "// BEP-044: interface fields are linked to class-owned fields with"
// field_alias.baml:2:1 (comment) len=36 "// `interface_field as class_field`."
// field_alias.baml:4:1 (keyword) len=9 "interface"
// field_alias.baml:4:11 (interface) [declaration] len=5 "Named"
// field_alias.baml:5:3 (property) [declaration] len=4 "name"
// field_alias.baml:5:9 (type) [defaultLibrary] len=6 "string"
// field_alias.baml:8:1 (keyword) len=5 "class"
// field_alias.baml:8:7 (class) [declaration] len=6 "Person"
// field_alias.baml:9:3 (property) [declaration] len=12 "display_name"
// field_alias.baml:9:17 (type) [defaultLibrary] len=6 "string"
// field_alias.baml:11:3 (keyword) len=10 "implements"
// field_alias.baml:11:14 (interface) len=5 "Named"
// field_alias.baml:12:5 (property) len=4 "name"
// field_alias.baml:12:10 (keyword) len=2 "as"
// field_alias.baml:12:13 (property) len=12 "display_name"
// field_alias.baml:16:1 (keyword) len=8 "function"
// field_alias.baml:16:10 (function) [declaration] len=9 "read_name"
// field_alias.baml:16:20 (parameter) [declaration] len=1 "p"
// field_alias.baml:16:23 (class) len=6 "Person"
// field_alias.baml:16:34 (type) [defaultLibrary] len=6 "string"
// field_alias.baml:17:3 (keyword) len=6 "return"
// field_alias.baml:17:10 (parameter) len=1 "p"
// field_alias.baml:17:12 (keyword) len=2 "as"
// field_alias.baml:17:14 (operator) len=1 "<"
// field_alias.baml:17:15 (interface) len=5 "Named"
// field_alias.baml:17:20 (operator) len=1 ">"
// field_alias.baml:17:22 (property) len=4 "name"
