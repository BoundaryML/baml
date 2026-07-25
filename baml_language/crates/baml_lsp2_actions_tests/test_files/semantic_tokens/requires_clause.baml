// BEP-044: fields contributed by a grandparent interface should flow into
// the implementor through the full requires chain.

interface Named { name: string }
interface Aged { age: int }
interface Person requires Named, Aged {
  occupation: string

  function introduce(self) -> string throws never {
    return self.name + ", " + self.occupation
  }

  function age_value(self) -> int throws never {
    return self.age
  }
}

class Employee {
  name: string
  age: int
  occupation: string
  salary: float

  implements Named {}
  implements Aged {}
  implements Person {}
}

function get_name(e: Employee) -> string {
  return e.name
}

function get_age(e: Employee) -> int {
  return e.age
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// requires_clause.baml:1:1 (comment) len=74 "// BEP-044: fields contributed by a grandparent interface should flow into"
// requires_clause.baml:2:1 (comment) len=51 "// the implementor through the full requires chain."
// requires_clause.baml:4:1 (keyword) len=9 "interface"
// requires_clause.baml:4:11 (interface) [declaration] len=5 "Named"
// requires_clause.baml:4:19 (property) [declaration] len=4 "name"
// requires_clause.baml:4:25 (type) [defaultLibrary] len=6 "string"
// requires_clause.baml:5:1 (keyword) len=9 "interface"
// requires_clause.baml:5:11 (interface) [declaration] len=4 "Aged"
// requires_clause.baml:5:18 (property) [declaration] len=3 "age"
// requires_clause.baml:5:23 (type) [defaultLibrary] len=3 "int"
// requires_clause.baml:6:1 (keyword) len=9 "interface"
// requires_clause.baml:6:11 (interface) [declaration] len=6 "Person"
// requires_clause.baml:6:18 (keyword) len=8 "requires"
// requires_clause.baml:6:27 (interface) len=5 "Named"
// requires_clause.baml:6:34 (interface) len=4 "Aged"
// requires_clause.baml:7:3 (property) [declaration] len=10 "occupation"
// requires_clause.baml:7:15 (type) [defaultLibrary] len=6 "string"
// requires_clause.baml:9:3 (keyword) len=8 "function"
// requires_clause.baml:9:12 (method) [declaration] len=9 "introduce"
// requires_clause.baml:9:22 (parameter) [declaration] len=4 "self"
// requires_clause.baml:9:31 (type) [defaultLibrary] len=6 "string"
// requires_clause.baml:9:38 (keyword) len=6 "throws"
// requires_clause.baml:9:45 (type) [defaultLibrary] len=5 "never"
// requires_clause.baml:10:5 (keyword) len=6 "return"
// requires_clause.baml:10:12 (parameter) len=4 "self"
// requires_clause.baml:10:17 (property) len=4 "name"
// requires_clause.baml:10:22 (operator) len=1 "+"
// requires_clause.baml:10:24 (string) len=4 "\", \""
// requires_clause.baml:10:29 (operator) len=1 "+"
// requires_clause.baml:10:31 (parameter) len=4 "self"
// requires_clause.baml:10:36 (property) len=10 "occupation"
// requires_clause.baml:13:3 (keyword) len=8 "function"
// requires_clause.baml:13:12 (method) [declaration] len=9 "age_value"
// requires_clause.baml:13:22 (parameter) [declaration] len=4 "self"
// requires_clause.baml:13:31 (type) [defaultLibrary] len=3 "int"
// requires_clause.baml:13:35 (keyword) len=6 "throws"
// requires_clause.baml:13:42 (type) [defaultLibrary] len=5 "never"
// requires_clause.baml:14:5 (keyword) len=6 "return"
// requires_clause.baml:14:12 (parameter) len=4 "self"
// requires_clause.baml:14:17 (property) len=3 "age"
// requires_clause.baml:18:1 (keyword) len=5 "class"
// requires_clause.baml:18:7 (class) [declaration] len=8 "Employee"
// requires_clause.baml:19:3 (property) [declaration] len=4 "name"
// requires_clause.baml:19:9 (type) [defaultLibrary] len=6 "string"
// requires_clause.baml:20:3 (property) [declaration] len=3 "age"
// requires_clause.baml:20:8 (type) [defaultLibrary] len=3 "int"
// requires_clause.baml:21:3 (property) [declaration] len=10 "occupation"
// requires_clause.baml:21:15 (type) [defaultLibrary] len=6 "string"
// requires_clause.baml:22:3 (property) [declaration] len=6 "salary"
// requires_clause.baml:22:11 (type) [defaultLibrary] len=5 "float"
// requires_clause.baml:24:3 (keyword) len=10 "implements"
// requires_clause.baml:24:14 (interface) len=5 "Named"
// requires_clause.baml:25:3 (keyword) len=10 "implements"
// requires_clause.baml:25:14 (interface) len=4 "Aged"
// requires_clause.baml:26:3 (keyword) len=10 "implements"
// requires_clause.baml:26:14 (interface) len=6 "Person"
// requires_clause.baml:29:1 (keyword) len=8 "function"
// requires_clause.baml:29:10 (function) [declaration] len=8 "get_name"
// requires_clause.baml:29:19 (parameter) [declaration] len=1 "e"
// requires_clause.baml:29:22 (class) len=8 "Employee"
// requires_clause.baml:29:35 (type) [defaultLibrary] len=6 "string"
// requires_clause.baml:30:3 (keyword) len=6 "return"
// requires_clause.baml:30:10 (parameter) len=1 "e"
// requires_clause.baml:30:12 (property) len=4 "name"
// requires_clause.baml:33:1 (keyword) len=8 "function"
// requires_clause.baml:33:10 (function) [declaration] len=7 "get_age"
// requires_clause.baml:33:18 (parameter) [declaration] len=1 "e"
// requires_clause.baml:33:21 (class) len=8 "Employee"
// requires_clause.baml:33:34 (type) [defaultLibrary] len=3 "int"
// requires_clause.baml:34:3 (keyword) len=6 "return"
// requires_clause.baml:34:10 (parameter) len=1 "e"
// requires_clause.baml:34:12 (property) len=3 "age"
