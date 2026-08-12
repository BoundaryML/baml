class Foo {
    bar int @stream.not_null
    baz string | int @stream.done
    quux bool @stream.with_state
}

class Foo2 {
    something int
    tomething_else bool
    @@stream.done
}

//----
//- diagnostics
// E0015
//
//   × unknown attribute `@stream.not_null`
//    ╭─[streaming_streaming_ok.baml:2:13]
//  2 │     bar int @stream.not_null
//    ·             ────────┬───────
//    ·                     ╰── unknown attribute
//    ╰────
