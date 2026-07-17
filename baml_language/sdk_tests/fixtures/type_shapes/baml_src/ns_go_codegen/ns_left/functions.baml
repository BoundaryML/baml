// This function and its sibling deliberately share a bare name. The Go SDK
// keeps both in one package and qualifies them with their BAML namespaces.
// The aliases also share a bare name, which exercises canonical qualified-name
// accessors before each host generator applies its namespace routing.
type EchoValue = string

function echo(value: EchoValue) -> EchoValue {
  value
}
