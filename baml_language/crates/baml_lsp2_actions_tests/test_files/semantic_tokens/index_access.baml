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
//    ╭─[index_access.baml:6:1]
//  6 │ ╭─▶ let foo = Foo {
//  7 │ │       name: "John",
//  8 │ │       age: 2,
//  9 │ ├─▶ };
//    · ╰──── move this binding into a function body
//    ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[index_access.baml:10:1]
//  10 │ let name = foo.name;
//     · ──────────┬─────────
//     ·           ╰── move this binding into a function body
//     ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[index_access.baml:11:1]
//  11 │ let misspelling = foo.names;
//     · ──────────────┬─────────────
//     ·               ╰── move this binding into a function body
//     ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[index_access.baml:12:1]
//  12 │ let nonsense = foo.name.field;
//     · ───────────────┬──────────────
//     ·                ╰── move this binding into a function body
//     ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[index_access.baml:14:1]
//  14 │ let array_access_1 = [1, 2, 3][0];
//     · ─────────────────┬────────────────
//     ·                  ╰── move this binding into a function body
//     ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[index_access.baml:15:1]
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     · ──────────────────┬─────────────────
//     ·                   ╰── move this binding into a function body
//     ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[index_access.baml:16:1]
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     · ───────────────────┬───────────────────
//     ·                    ╰── move this binding into a function body
//     ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//     ╭─[index_access.baml:17:1]
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     · ────────────────────┬───────────────────
//     ·                     ╰── move this binding into a function body
//     ╰────
//
//- semantic_tokens
// index_access.baml:1:1 (keyword) len=5 "class"
// index_access.baml:1:7 (class) [declaration] len=3 "Foo"
// index_access.baml:2:5 (property) [declaration] len=4 "name"
// index_access.baml:2:10 (type) [defaultLibrary] len=6 "string"
// index_access.baml:3:5 (property) [declaration] len=3 "age"
// index_access.baml:3:9 (type) [defaultLibrary] len=3 "int"
// index_access.baml:6:1 (keyword) len=3 "let"
// index_access.baml:6:5 (variable) [declaration] len=3 "foo"
// index_access.baml:6:9 (operator) len=1 "="
// index_access.baml:6:11 (class) len=3 "Foo"
// index_access.baml:7:5 (property) len=4 "name"
// index_access.baml:7:11 (string) len=6 "\"John\""
// index_access.baml:8:5 (property) len=3 "age"
// index_access.baml:8:10 (number) len=1 "2"
// index_access.baml:10:1 (keyword) len=3 "let"
// index_access.baml:10:5 (variable) [declaration] len=4 "name"
// index_access.baml:10:10 (operator) len=1 "="
// index_access.baml:11:1 (keyword) len=3 "let"
// index_access.baml:11:5 (variable) [declaration] len=11 "misspelling"
// index_access.baml:11:17 (operator) len=1 "="
// index_access.baml:12:1 (keyword) len=3 "let"
// index_access.baml:12:5 (variable) [declaration] len=8 "nonsense"
// index_access.baml:12:14 (operator) len=1 "="
// index_access.baml:14:1 (keyword) len=3 "let"
// index_access.baml:14:5 (variable) [declaration] len=14 "array_access_1"
// index_access.baml:14:20 (operator) len=1 "="
// index_access.baml:14:23 (number) len=1 "1"
// index_access.baml:14:26 (number) len=1 "2"
// index_access.baml:14:29 (number) len=1 "3"
// index_access.baml:14:32 (number) len=1 "0"
// index_access.baml:15:1 (keyword) len=3 "let"
// index_access.baml:15:5 (variable) [declaration] len=14 "array_access_2"
// index_access.baml:15:20 (operator) len=1 "="
// index_access.baml:15:23 (number) len=1 "1"
// index_access.baml:15:25 (number) len=1 "2"
// index_access.baml:15:28 (number) len=1 "3"
// index_access.baml:15:31 (string) len=4 "\"hi\""
// index_access.baml:16:1 (keyword) len=3 "let"
// index_access.baml:16:5 (variable) [declaration] len=14 "array_access_3"
// index_access.baml:16:20 (operator) len=1 "="
// index_access.baml:16:23 (number) len=1 "1"
// index_access.baml:16:25 (number) len=1 "2"
// index_access.baml:16:28 (number) len=1 "3"
// index_access.baml:17:1 (keyword) len=3 "let"
// index_access.baml:17:5 (variable) [declaration] len=14 "array_access_4"
// index_access.baml:17:20 (operator) len=1 "="
// index_access.baml:17:23 (number) len=1 "1"
// index_access.baml:17:25 (number) len=1 "2"
// index_access.baml:17:28 (number) len=1 "3"
// index_access.baml:19:1 (comment) len=56 "// error: Error validating: Class Foo has no field names"
// index_access.baml:20:1 (comment) len=29 "//   -->  expr/access.baml:11"
// index_access.baml:21:1 (comment) len=7 "//    |"
// index_access.baml:22:1 (comment) len=28 "// 10 | let name = foo.name;"
// index_access.baml:23:1 (comment) len=36 "// 11 | let misspelling = foo.names;"
// index_access.baml:24:1 (comment) len=7 "//    |"
// index_access.baml:25:1 (comment) len=69 "// error: Error validating: Can only access fields on class instances"
// index_access.baml:26:1 (comment) len=29 "//   -->  expr/access.baml:12"
// index_access.baml:27:1 (comment) len=7 "//    |"
// index_access.baml:28:1 (comment) len=36 "// 11 | let misspelling = foo.names;"
// index_access.baml:29:1 (comment) len=38 "// 12 | let nonsense = foo.name.field;"
// index_access.baml:30:1 (comment) len=7 "//    |"
// index_access.baml:31:1 (comment) len=55 "// error: Error validating: Array index must be integer"
// index_access.baml:32:1 (comment) len=29 "//   -->  expr/access.baml:15"
// index_access.baml:33:1 (comment) len=7 "//    |"
// index_access.baml:34:1 (comment) len=42 "// 14 | let array_access_1 = [1, 2, 3][0];"
// index_access.baml:35:1 (comment) len=44 "// 15 | let array_access_2 = [1,2, 3][\"hi\"];"
// index_access.baml:36:1 (comment) len=7 "//    |"
// index_access.baml:37:1 (comment) len=55 "// error: Error validating: Array index must be integer"
// index_access.baml:38:1 (comment) len=29 "//   -->  expr/access.baml:17"
// index_access.baml:39:1 (comment) len=7 "//    |"
// index_access.baml:40:1 (comment) len=47 "// 16 | let array_access_3 = [1,2, 3][foo.age];"
// index_access.baml:41:1 (comment) len=48 "// 17 | let array_access_4 = [1,2, 3][foo.name];"
// index_access.baml:42:1 (comment) len=7 "//    |"
