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
// Error: Expected field name or '}', found Dot
//    ╭─[ 0:8:25 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │                         ┬  
//    │                         ╰── Expected field name or '}', found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected field name or '}', found Dot
//    ╭─[ 0:8:26 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │                          ┬  
//    │                          ╰── Expected field name or '}', found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected field name or '}', found Dot
//    ╭─[ 0:8:27 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │                           ┬  
//    │                           ╰── Expected field name or '}', found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Colon, found RBrace
//    ╭─[ 0:8:30 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │                              ┬  
//    │                              ╰── Expected Colon, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
