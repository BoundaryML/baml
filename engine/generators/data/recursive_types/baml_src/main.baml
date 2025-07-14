type JSON = string | null | int | float | map<string, JSON> | JSON[]

function Foo(x: int) -> JSON {
    client "openai/gpt-4o-mini"
    prompt #"
        print out a json object with the following fields:
        - a: {{ x }}
        - b: "hello"
        - c: [1, 2, 3]
        - d: { "e": ["f"] }
        - f: null
        - g: 1.0
        - h: { a: 1, b: null, c: [1, 2, 3] }
        
        {{ ctx.output_format }}
    "#
}

function JsonInput(x: JSON) -> JSON {
    client "openai/gpt-4o-mini"
    prompt #"
        repeat back to me:
        {{ x }}
        
        {{ ctx.output_format }}
    "#
}

// This union uses a mix of recursive and non-recursive types.
// It is meant to test that codegeneration simplifies to
// a smaller union before generation of a Go type.
//
// Should generate a union in the client like:
// type Recursive1 = int | Recursive1[]
// int | Recursive1[] | string | null
type MyUnion = Recursive1 | Nonrecursive1 | Nonrecursive2

type Recursive1 = int | Recursive1[]

type Nonrecursive1 = int | null

type Nonrecursive2 = (null | string) | null | (null | null)

class UseMyUnion {
    u MyUnion
}
