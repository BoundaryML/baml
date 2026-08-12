interface Animal {
    function speak(self) -> string throws never
}

class Dog<[CURSOR] {
    name: string
}

implements Animal for Dog {
    function speak(self) -> string {
        return self.name
    }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at hover_out_of_body_implements_class.baml:5:10
// ```baml
// class Dog {
//     name: string,
//     implements Animal {}
// }
// ```
//
// Run `baml describe Dog` for methods and details.
