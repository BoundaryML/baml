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