interface Named {
  display_na<[CURSOR]me string
  function dis<[CURSOR]play(self) -> string throws never
}

class Person {
  na<[CURSOR]me string

  function gree<[CURSOR]ting(self) -> string {
    return self.name
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at b_977_method_and_field_declarations.baml:2:13
// ```baml
// display_name: string
// ```
// hover at b_977_method_and_field_declarations.baml:3:15
// ```baml
// function display(self) -> string throws never
// ```
// hover at b_977_method_and_field_declarations.baml:7:5
// ```baml
// name: string
// ```
// hover at b_977_method_and_field_declarations.baml:9:16
// ```baml
// function greeting(self) -> string throws never
// ```
