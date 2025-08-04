class Foo {
    name string
    // aliases string[]
    // pet Pet
    // color_preferences map<string, string>
}
class Pet {
    name string
    age int
}

// let foo = Foo {
//     name: "John",
//     pet: Pet {
//         name: "Buddy",
//         age: 3,
//     },
//     aliases: ["John", "Johnny"],
//     color_preferences: {
//         blue "awesome"
//         red "ok"
//         green "sweet"
//     }
// };
let foo = Foo {
    name: "John"
};
let name = foo.name;

// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/access.baml:25
//    | 
// 24 | // };
// 25 | let foo = Foo {
// 26 |     name: "John"
// 27 | };
//    | 
// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/access.baml:28
//    | 
// 27 | };
// 28 | let name = foo.name;
//    | 