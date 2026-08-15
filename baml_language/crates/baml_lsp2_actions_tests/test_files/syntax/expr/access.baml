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
// E0010
//
//   × top-level `let` bindings are not supported
//    ╭─[expr_access.baml:6:1]
//  6 │ ╭─▶ let foo = Foo {
//  7 │ │       name: "John",
//  8 │ │       age: 2,
//  9 │ ├─▶ };
//    · ╰──── move this binding into a function body
//    ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[expr_access.baml:10:1]
//  10 │ let name = foo.name;
//     · ──────────┬─────────
//     ·           ╰── move this binding into a function body
//     ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[expr_access.baml:11:1]
//  11 │ let misspelling = foo.names;
//     · ──────────────┬─────────────
//     ·               ╰── move this binding into a function body
//     ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[expr_access.baml:12:1]
//  12 │ let nonsense = foo.name.field;
//     · ───────────────┬──────────────
//     ·                ╰── move this binding into a function body
//     ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[expr_access.baml:14:1]
//  14 │ let array_access_1 = [1, 2, 3][0];
//     · ─────────────────┬────────────────
//     ·                  ╰── move this binding into a function body
//     ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[expr_access.baml:15:1]
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     · ──────────────────┬─────────────────
//     ·                   ╰── move this binding into a function body
//     ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[expr_access.baml:16:1]
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     · ───────────────────┬───────────────────
//     ·                    ╰── move this binding into a function body
//     ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[expr_access.baml:17:1]
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     · ────────────────────┬───────────────────
//     ·                     ╰── move this binding into a function body
//     ╰────
