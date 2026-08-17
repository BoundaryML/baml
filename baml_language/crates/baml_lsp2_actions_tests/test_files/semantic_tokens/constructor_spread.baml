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
//    ╭─[constructor_spread.baml:6:1]
//  6 │ let x = MyClass { a: 1, b: "2" };
//    · ────────────────┬────────────────
//    ·                 ╰── move this binding into a function body
//    ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//    ╭─[constructor_spread.baml:8:1]
//  8 │ let y = MyClass { a: 1, ...x };
//    · ───────────────┬───────────────
//    ·                ╰── move this binding into a function body
//    ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[constructor_spread.baml:11:1]
//  11 │ ╭─▶ let default_person = Person {
//  12 │ │       name: "John Doe",
//  13 │ │       age: 20,
//  14 │ │       poem: "Never was there a man more plain."
//  15 │ ├─▶ };
//     · ╰──── move this binding into a function body
//     ╰────
//
//- semantic_tokens
// constructor_spread.baml:1:1 (keyword) len=5 "class"
// constructor_spread.baml:1:7 (class) [declaration] len=7 "MyClass"
// constructor_spread.baml:2:5 (property) [declaration] len=1 "a"
// constructor_spread.baml:2:7 (type) [defaultLibrary] len=3 "int"
// constructor_spread.baml:3:5 (property) [declaration] len=1 "b"
// constructor_spread.baml:3:7 (type) [defaultLibrary] len=6 "string"
// constructor_spread.baml:6:1 (keyword) len=3 "let"
// constructor_spread.baml:6:5 (variable) [declaration] len=1 "x"
// constructor_spread.baml:6:7 (operator) len=1 "="
// constructor_spread.baml:6:9 (class) len=7 "MyClass"
// constructor_spread.baml:6:19 (property) len=1 "a"
// constructor_spread.baml:6:22 (number) len=1 "1"
// constructor_spread.baml:6:25 (property) len=1 "b"
// constructor_spread.baml:6:28 (string) len=3 "\"2\""
// constructor_spread.baml:8:1 (keyword) len=3 "let"
// constructor_spread.baml:8:5 (variable) [declaration] len=1 "y"
// constructor_spread.baml:8:7 (operator) len=1 "="
// constructor_spread.baml:8:9 (class) len=7 "MyClass"
// constructor_spread.baml:8:19 (property) len=1 "a"
// constructor_spread.baml:8:22 (number) len=1 "1"
// constructor_spread.baml:11:1 (keyword) len=3 "let"
// constructor_spread.baml:11:5 (variable) [declaration] len=14 "default_person"
// constructor_spread.baml:11:20 (operator) len=1 "="
// constructor_spread.baml:11:22 (class) len=6 "Person"
// constructor_spread.baml:12:5 (property) len=4 "name"
// constructor_spread.baml:12:11 (string) len=10 "\"John Doe\""
// constructor_spread.baml:13:5 (property) len=3 "age"
// constructor_spread.baml:13:10 (number) len=2 "20"
// constructor_spread.baml:14:5 (property) len=4 "poem"
// constructor_spread.baml:14:11 (string) len=35 "\"Never was there a man more plain.\""
// constructor_spread.baml:18:1 (keyword) len=5 "class"
// constructor_spread.baml:18:7 (class) [declaration] len=6 "Person"
// constructor_spread.baml:19:3 (property) [declaration] len=4 "name"
// constructor_spread.baml:19:8 (type) [defaultLibrary] len=6 "string"
// constructor_spread.baml:20:3 (property) [declaration] len=3 "age"
// constructor_spread.baml:20:7 (type) [defaultLibrary] len=3 "int"
// constructor_spread.baml:21:3 (property) [declaration] len=4 "poem"
// constructor_spread.baml:21:8 (type) [defaultLibrary] len=6 "string"
