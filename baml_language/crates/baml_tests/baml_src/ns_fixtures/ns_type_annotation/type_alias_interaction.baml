// Test interaction between `type` as a declaration keyword and `reflect.Type`

// Type alias whose value is the meta-type
type MetaAlias = reflect.Type

// Type alias whose value is a list of meta-types
type MetaList = reflect.Type[]

// Type alias whose value is a union including the meta-type
type MetaUnion = reflect.Type | string
