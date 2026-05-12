template_string Greeting(name: string = "world") #"hello {{ name }}"#

type NamedFilter = (query: string, filter?: string?) -> int
type NamedLimit = (query: string, limit?: string?) -> int

class Counter {
  value int

  function Current(self = null) -> int {
    self.value
  }
}

function LambdaDefault() -> int {
  let f = (x: int = 1) -> int { x }
  f(2)
}

function Filtered(query: string, filter: string? = null) -> int {
  query.length()
}

function OptionalNameMismatch() -> int {
  let f: NamedLimit = Filtered
  f("cats", limit = "recent")
}
