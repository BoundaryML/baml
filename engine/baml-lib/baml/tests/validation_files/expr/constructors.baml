class MyClass {
    a int
    b string
}

let x = MyClass { a: 1, b: 2 };

let b = MyClass { a: 4, ..x };