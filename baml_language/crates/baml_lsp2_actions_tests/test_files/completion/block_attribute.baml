class Person {
    name string
    @@<[CURSOR]
}

//----
//- diagnostics
// (parse error expected - incomplete block attribute)
// E0010
//
//   × unexpected token
//    ╭─[block_attribute.baml:4:1]
//  4 │ }
//    · ┬
//    · ╰── expected `attribute name`, found `'}'`
//    ╰────
