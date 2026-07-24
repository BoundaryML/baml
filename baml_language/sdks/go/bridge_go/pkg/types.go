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

// DynamicUnion represents a BAML union variant. Variant is display-only;
// SelectedOptionIndex is the canonical arm identity when present.
type DynamicUnion struct {
	Variant             string
	SelectedOptionIndex *uint32
	Value               any
}
