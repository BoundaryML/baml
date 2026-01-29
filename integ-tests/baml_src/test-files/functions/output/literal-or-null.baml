class ClassForNullLiteral {
  a "hi"
}

function NullLiteralClassHello(s: string) -> ClassForNullLiteral {
    client GPT35
    prompt #"
        Return the empty object: {}.
    "#
}

function NullLiteralClassHelloExprFunc(s: string) -> ClassForNullLiteral {
    return ClassForNullLiteral {
        a: "hi"
    };
}

test NullLiteralClassHello {
    functions [NullLiteralClassHello]
    args { s "unused" }
}