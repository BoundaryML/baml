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
