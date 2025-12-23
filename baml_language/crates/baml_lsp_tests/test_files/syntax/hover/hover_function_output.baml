class Person {
    name string
    age int
}

function GetPerson() -> Person {
// on_hover `Person`: class Person {
//   name string
//   age int
// }
    Person { name: "Alice", age: 30 }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// `Person` at hover_hover_function_output.baml:6
// class Person {
//   name string
//   age int
// }
