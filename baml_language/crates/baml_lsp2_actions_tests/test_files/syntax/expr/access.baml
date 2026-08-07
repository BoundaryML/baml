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

//----
//- diagnostics
// E0007
//
//   × type `Foo` has no member `names`
//     ╭─[expr_access.baml:11:19]
//  11 │ let misspelling = foo.names;
//     ·                   ─────────
//     ╰────
//   ╰─▶   ☞ class defined here
//          ╭─[expr_access.baml:1:7]
//        1 │ class Foo {
//          ·       ───
//          ╰────
// E0007
//
//   × type `string` has no member `field`
//     ╭─[expr_access.baml:12:16]
//  12 │ let nonsense = foo.name.field;
//     ·                ──────────────
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[expr_access.baml:15:31]
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     ·                               ──┬─
//     ·                                 ╰── expected `int`, found `string`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[expr_access.baml:17:31]
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     ·                               ────┬───
//     ·                                   ╰── expected `int`, found `string`
//     ╰────
