// fmt: line_width=60
class Person {
    /// A documentation line that is far longer than sixty columns on its own.
    name string @alias("n") @description("the name")
}

enum Status {
    /// A documentation line that is far longer than sixty columns on its own.
    Unsupported /*c*/ @alias("u")
}
