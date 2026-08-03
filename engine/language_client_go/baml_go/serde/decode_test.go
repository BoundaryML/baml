package serde

import (
	"reflect"
	"testing"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
	"github.com/ghetzel/testify/assert"
	"github.com/ghetzel/testify/require"
)

type nilSentinel struct{}

func untypedTypeMap() TypeMap {
	return NewInternalTypeMap(map[string]reflect.Type{
		"INTERNAL.nil": reflect.TypeOf((*nilSentinel)(nil)).Elem(),
	})
}

func stringFieldType() *cffi.CFFIFieldTypeHolder {
	return &cffi.CFFIFieldTypeHolder{
		Type: &cffi.CFFIFieldTypeHolder_StringType{StringType: &cffi.CFFIFieldTypeString{}},
	}
}

// untypedFieldType returns a field type the internal type map can't resolve,
// which convertFieldTypeToGoType collapses to INTERNAL.nil. That is the shape
// the engine emits for a list whose elements don't share a static type.
func untypedFieldType() *cffi.CFFIFieldTypeHolder {
	return &cffi.CFFIFieldTypeHolder{
		Type: &cffi.CFFIFieldTypeHolder_UnionVariantType{
			UnionVariantType: &cffi.CFFIFieldTypeUnionVariant{
				Name: &cffi.CFFITypeName{Namespace: cffi.CFFITypeNamespace_TYPES, Name: "Unresolved"},
			},
		},
	}
}

func stringValue(s string) *cffi.CFFIValueHolder {
	return &cffi.CFFIValueHolder{Value: &cffi.CFFIValueHolder_StringValue{StringValue: s}}
}

func nullValue() *cffi.CFFIValueHolder {
	return &cffi.CFFIValueHolder{Value: &cffi.CFFIValueHolder_NullValue{}}
}

func objectValue(fields map[string]string) *cffi.CFFIValueHolder {
	entries := make([]*cffi.CFFIMapEntry, 0, len(fields))
	for key, value := range fields {
		entries = append(entries, &cffi.CFFIMapEntry{Key: key, Value: stringValue(value)})
	}
	return &cffi.CFFIValueHolder{Value: &cffi.CFFIValueHolder_MapValue{MapValue: &cffi.CFFIValueMap{
		KeyType:   stringFieldType(),
		ValueType: stringFieldType(),
		Entries:   entries,
	}}}
}

func untypedList(items ...*cffi.CFFIValueHolder) *cffi.CFFIValueHolder {
	return &cffi.CFFIValueHolder{Value: &cffi.CFFIValueHolder_ListValue{ListValue: &cffi.CFFIValueList{
		ItemType: untypedFieldType(),
		Items:    items,
	}}}
}

func TestDecodeUntypedListOfObjects(t *testing.T) {
	holder := untypedList(
		objectValue(map[string]string{"type": "text", "text": "abcdef"}),
		objectValue(map[string]string{"type": "text", "text": "ghijkl", "cache_control": "ephemeral"}),
	)

	var decoded reflect.Value
	require.NotPanics(t, func() {
		decoded, _ = Decode(holder, untypedTypeMap())
	})

	assert.Equal(t, []any{
		map[string]string{"type": "text", "text": "abcdef"},
		map[string]string{"type": "text", "text": "ghijkl", "cache_control": "ephemeral"},
	}, decoded.Interface())
}

func TestDecodeUntypedListOfScalars(t *testing.T) {
	holder := untypedList(stringValue("a"), stringValue("b"))

	var decoded reflect.Value
	require.NotPanics(t, func() {
		decoded, _ = Decode(holder, untypedTypeMap())
	})

	assert.Equal(t, []any{"a", "b"}, decoded.Interface())
}

func TestDecodeUntypedListWithNullElement(t *testing.T) {
	holder := untypedList(stringValue("a"), nullValue())

	var decoded reflect.Value
	require.NotPanics(t, func() {
		decoded, _ = Decode(holder, untypedTypeMap())
	})

	assert.Equal(t, []any{"a", nil}, decoded.Interface())
}
