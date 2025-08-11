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

// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/access.baml:6
//    | 
//  5 | 
//  6 | let foo = Foo {
//  7 |     name: "John",
//  8 |     age: 2,
//  9 | };
//    | 
// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/access.baml:10
//    | 
//  9 | };
// 10 | let name = foo.name;
//    | 
// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/access.baml:11
//    | 
// 10 | let name = foo.name;
// 11 | let misspelling = foo.names;
//    | 
// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/access.baml:12
//    | 
// 11 | let misspelling = foo.names;
// 12 | let nonsense = foo.name.field;
//    | 
// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/access.baml:14
//    | 
// 13 | 
// 14 | let array_access_1 = [1, 2, 3][0];
//    | 
// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/access.baml:15
//    | 
// 14 | let array_access_1 = [1, 2, 3][0];
// 15 | let array_access_2 = [1,2, 3]["hi"];
//    | 
// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/access.baml:16
//    | 
// 15 | let array_access_2 = [1,2, 3]["hi"];
// 16 | let array_access_3 = [1,2, 3][foo.age];
//    | 
// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/access.baml:17
//    | 
// 16 | let array_access_3 = [1,2, 3][foo.age];
// 17 | let array_access_4 = [1,2, 3][foo.name];
//    | 
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
