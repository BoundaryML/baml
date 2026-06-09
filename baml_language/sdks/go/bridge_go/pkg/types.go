package pkg

// DynamicEnum represents a BAML enum value with its type name.
type DynamicEnum struct {
	Name  string
	Value string
}

// DynamicClass represents a BAML class value with its type name and fields.
type DynamicClass struct {
	Name   string
	Fields map[string]any
}

// DynamicUnion represents a BAML union variant with the selected variant name and value.
type DynamicUnion struct {
	Variant string
	Value   any
}
