function Search(query: string, max_results: int = 10, filter: string = "none") -> string {
  query
}

function OkOmit() -> string {
  Search("cats")
}

function OkNamedDefault() -> string {
  Search("cats", max_results = 5)
}

function OkNamedRequired() -> string {
  Search(query = "cats")
}

function OkSkipFirstDefault() -> string {
  Search("cats", filter = "recent")
}

function BadDefaultPositional() -> string {
  Search("cats", 5)
}

function BadPositionalAfterNamed() -> string {
  Search(query = "cats", 5)
}

function BadDuplicateNamed() -> string {
  Search(query = "cats", max_results = 1, max_results = 2)
}

function BadUnknownNamed() -> string {
  Search(q = "cats")
}

function BadForwardRef(a: int = b, b: int = 1) -> int {
  a
}

function BadRequiredAfterDefault(a: int = 1, b: int) -> int {
  b
}
