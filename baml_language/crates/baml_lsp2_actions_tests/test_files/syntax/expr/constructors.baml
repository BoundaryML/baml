class MyClass {
    a int
    b string
}

let x = MyClass { a: 1, b: "2" };

let y = MyClass { a: 1, ...x };


let default_person = Person {
    name: "John Doe",
    age: 20,
    poem: "Never was there a man more plain."
};


class Person {
  name string
  age int
  poem string
}

//----
//- diagnostics
// E0010
//
//   × top-level `let` bindings are not supported
//    ╭─[expr_constructors.baml:6:1]
//  6 │ let x = MyClass { a: 1, b: "2" };
//    · ────────────────┬────────────────
//    ·                 ╰── move this binding into a function body
//    ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//    ╭─[expr_constructors.baml:8:1]
//  8 │ let y = MyClass { a: 1, ...x };
//    · ───────────────┬───────────────
//    ·                ╰── move this binding into a function body
//    ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[expr_constructors.baml:11:1]
//  11 │ ╭─▶ let default_person = Person {
//  12 │ │       name: "John Doe",
//  13 │ │       age: 20,
//  14 │ │       poem: "Never was there a man more plain."
//  15 │ ├─▶ };
//     · ╰──── move this binding into a function body
//     ╰────
