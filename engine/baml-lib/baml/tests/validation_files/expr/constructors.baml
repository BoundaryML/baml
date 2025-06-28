class MyClass {
    a int
    b string
}

let x = MyClass { a: 1, b: "2" };

let y = MyClass { a: 1, ..x };


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

// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/constructors.baml:6
//    | 
//  5 | 
//  6 | let x = MyClass { a: 1, b: "2" };
//    | 
// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/constructors.baml:8
//    | 
//  7 | 
//  8 | let y = MyClass { a: 1, ..x };
//    | 
// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/constructors.baml:11
//    | 
// 10 | 
// 11 | let default_person = Person {
// 12 |     name: "John Doe",
// 13 |     age: 20,
// 14 |     poem: "Never was there a man more plain."
// 15 | };
//    | 
