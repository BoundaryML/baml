// Test interaction between `type` as keyword (type alias decl) and `type` as meta-type

// Type alias whose value is the meta-type
type MetaAlias = type

// Type alias whose value is a list of meta-types
type MetaList = type[]

// Type alias whose value is a union including the meta-type
type MetaUnion = type | string
