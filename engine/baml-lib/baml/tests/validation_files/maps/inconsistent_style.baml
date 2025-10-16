function InconsistentStyleUse() -> int{
    let some_ident = "world";

    // here, 'name' is a string, but 'some_ident' is an expression.
    // This cannot be discriminated by a context-free parser, so we'll rule it out via
    // a diagnostic. We could support it anyway with context-sensitive features like typing + name resolution.
    let map = {
        name "Hello"
        some_ident: "yes"
    };

    0
}

// error: Error validating: Inconsistent use of key-value pair syntax. Consider using python-style if any of the keys is an identifier to avoid confusion
//   -->  maps/inconsistent_style.baml:9
//    | 
//  8 |         name "Hello"
//  9 |         some_ident: "yes"
//    | 
