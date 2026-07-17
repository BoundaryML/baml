// These two namespaces normalize to the same Go prefix. Both functions must
// survive deterministic collision allocation and compile in one Go package.
function same_name(value: string) -> string {
  value
}
