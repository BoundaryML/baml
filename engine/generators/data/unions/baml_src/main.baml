type SystemComponentCategory = "service" | "resource"

class ExistingSystemComponent {
  id int
  name string
  type string
  category SystemComponentCategory
  explanation string
}

function JsonInput(x: ExistingSystemComponent[]) -> (string @assert({{ this|length > 0 and this[0] != "[" }}) @stream.not_null)[] @stream.not_null {
    client "ollama/phi4:latest"
    prompt #"
        repeat back to me a summary of this:
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
