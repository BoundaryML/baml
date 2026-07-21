package baml_go

import (
	"fmt"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// Enum encodes one statically declared BAML enum variant. Generated codecs
// supply every legal variant name so arbitrary values of the Go string-backed
// enum type cannot cross the boundary.
func Enum(name string, value string, variants ...string) Input {
	if name == "" {
		return Input{err: fmt.Errorf("BAML enum name is empty")}
	}
	if !containsEnumVariant(variants, value) {
		return Input{err: fmt.Errorf("invalid BAML enum %q variant %q", name, value)}
	}
	return Input{value: &cffi.InboundValue{Value: &cffi.InboundValue_EnumValue{
		EnumValue: &cffi.InboundEnumValue{Name: name, Value: value},
	}}}
}

// Enum validates and returns one statically declared BAML enum variant.
// Generated decoders convert the returned exact wire name to their named Go
// string type only after this validation succeeds.
func (value Value) Enum(name string, variants ...string) (string, error) {
	if value.value == nil {
		return "", fmt.Errorf("BAML value is uninitialized")
	}
	item, ok := value.value.Value.(*cffi.BamlOutboundValue_EnumValue)
	if !ok || item.EnumValue == nil {
		return "", fmt.Errorf("expected BAML enum %q, got %T", name, value.value.Value)
	}
	if item.EnumValue.Name != name {
		return "", fmt.Errorf("expected BAML enum %q, got %q", name, item.EnumValue.Name)
	}
	if item.EnumValue.IsDynamic {
		return "", fmt.Errorf(
			"BAML enum %q returned dynamic variant %q for a closed enum",
			name,
			item.EnumValue.Value,
		)
	}
	if !containsEnumVariant(variants, item.EnumValue.Value) {
		return "", fmt.Errorf(
			"BAML enum %q returned unknown variant %q",
			name,
			item.EnumValue.Value,
		)
	}
	return item.EnumValue.Value, nil
}

func containsEnumVariant(variants []string, value string) bool {
	for _, variant := range variants {
		if value == variant {
			return true
		}
	}
	return false
}
