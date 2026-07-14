// This function and its sibling deliberately share a bare name. The Go SDK
// keeps both in one package and qualifies them with their BAML namespaces.
function echo(value: string) -> string {
  value
}
