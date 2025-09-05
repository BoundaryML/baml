class Foo {
    name string
    age int
}

let foo = Foo {
    name: "John",
    age: 2,
};
let name = foo.name;
let misspelling = foo.names;
let nonsense = foo.name.field;

let array_access_1 = [1, 2, 3][0];
let array_access_2 = [1,2, 3]["hi"];
let array_access_3 = [1,2, 3][foo.age];
let array_access_4 = [1,2, 3][foo.name];

// error: Error validating: Class Foo has no field names
//   -->  expr/access.baml:11
//    | 
// 10 | let name = foo.name;
// 11 | let misspelling = foo.names;
//    | 
// error: Error validating: Can only access fields on class instances
//   -->  expr/access.baml:12
//    | 
// 11 | let misspelling = foo.names;
// 12 | let nonsense = foo.name.field;
//    | 
// error: Error validating: Array index must be integer
//   -->  expr/access.baml:15
//    | 
// 14 | let array_access_1 = [1, 2, 3][0];
// 15 | let array_access_2 = [1,2, 3]["hi"];
//    | 
// error: Error validating: Array index must be integer
//   -->  expr/access.baml:17
//    | 
// 16 | let array_access_3 = [1,2, 3][foo.age];
// 17 | let array_access_4 = [1,2, 3][foo.name];
//    | 
