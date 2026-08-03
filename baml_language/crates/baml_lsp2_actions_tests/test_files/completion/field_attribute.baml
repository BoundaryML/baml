class Person {
    name @<[CURSOR] string
}

//----
//- diagnostics
// (parse error expected - incomplete attribute)
// E0010
//
//   × invalid syntax
//    ╭─[field_attribute.baml:2:5]
//  2 │     name @ string
//    ·     ──┬─
//    ·       ╰── field 'name' is missing a type annotation
//    ╰────
// E0010
//
//   × unexpected token
//    ╭─[field_attribute.baml:2:10]
//  2 │     name @ string
//    ·          ┬
//    ·          ╰── expected `Unexpected token in class body`, found `'@'`
//    ╰────
// E0010
//
//   × invalid syntax
//    ╭─[field_attribute.baml:2:12]
//  2 │     name @ string
//    ·            ───┬──
//    ·               ╰── field 'string' is missing a type annotation
//    ╰────
