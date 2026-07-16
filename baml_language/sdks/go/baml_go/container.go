package baml_go

import (
	"fmt"
	"sort"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// List encodes a present BAML list. A nil Go slice is a present empty list;
// nullable lists use Optional around ListEncoder to represent BAML null.
func List[T any](values []T, encode func(T) Input) Input {
	items := make([]*cffi.InboundValue, 0, len(values))
	for _, value := range values {
		encoded := encode(value)
		if encoded.err != nil {
			return encoded
		}
		if encoded.value == nil {
			return Input{}
		}
		items = append(items, encoded.value)
	}
	return Input{value: &cffi.InboundValue{Value: &cffi.InboundValue_ListValue{
		ListValue: &cffi.InboundListValue{Values: items},
	}}}
}

// ListEncoder adapts an element encoder into the shape used by nested and
// nullable generated codecs.
func ListEncoder[T any](encode func(T) Input) func([]T) Input {
	return func(values []T) Input { return List(values, encode) }
}

// Map encodes a present BAML map with string keys. Entries are sorted so the
// protobuf representation is deterministic. A nil Go map is a present empty
// map; nullable maps use Optional around MapEncoder.
func Map[T any](values map[string]T, encode func(T) Input) Input {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	entries := make([]*cffi.InboundMapEntry, 0, len(keys))
	for _, key := range keys {
		encoded := encode(values[key])
		if encoded.err != nil {
			return encoded
		}
		if encoded.value == nil {
			return Input{}
		}
		entries = append(entries, &cffi.InboundMapEntry{
			Key:   &cffi.InboundMapEntry_StringKey{StringKey: key},
			Value: encoded.value,
		})
	}
	return Input{value: &cffi.InboundValue{Value: &cffi.InboundValue_MapValue{
		MapValue: &cffi.InboundMapValue{Entries: entries},
	}}}
}

// MapEncoder adapts a value encoder into the shape used by nested and
// nullable generated codecs.
func MapEncoder[T any](encode func(T) Input) func(map[string]T) Input {
	return func(values map[string]T) Input { return Map(values, encode) }
}

func DecodeList[T any](value Value, decode func(Value) (T, error)) ([]T, error) {
	if value.value == nil {
		return nil, fmt.Errorf("BAML value is uninitialized")
	}
	item, ok := value.value.Value.(*cffi.BamlOutboundValue_ListValue)
	if !ok || item.ListValue == nil {
		return nil, fmt.Errorf("expected BAML list, got %T", value.value.Value)
	}

	decoded := make([]T, len(item.ListValue.Items))
	for index, encoded := range item.ListValue.Items {
		if encoded == nil {
			return nil, fmt.Errorf("BAML list item %d is empty", index)
		}
		item, err := decode(Value{value: encoded})
		if err != nil {
			return nil, fmt.Errorf("BAML list item %d: %w", index, err)
		}
		decoded[index] = item
	}
	return decoded, nil
}

func ListDecoder[T any](decode func(Value) (T, error)) func(Value) ([]T, error) {
	return func(value Value) ([]T, error) { return DecodeList(value, decode) }
}

func DecodeMap[T any](value Value, decode func(Value) (T, error)) (map[string]T, error) {
	if value.value == nil {
		return nil, fmt.Errorf("BAML value is uninitialized")
	}
	item, ok := value.value.Value.(*cffi.BamlOutboundValue_MapValue)
	if !ok || item.MapValue == nil {
		return nil, fmt.Errorf("expected BAML map, got %T", value.value.Value)
	}

	decoded := make(map[string]T, len(item.MapValue.Entries))
	for index, entry := range item.MapValue.Entries {
		if entry == nil {
			return nil, fmt.Errorf("BAML map entry %d is empty", index)
		}
		if entry.Value == nil {
			return nil, fmt.Errorf("BAML map entry %q has an empty value", entry.Key)
		}
		if _, duplicate := decoded[entry.Key]; duplicate {
			return nil, fmt.Errorf("BAML map returned duplicate key %q", entry.Key)
		}
		mapValue, err := decode(Value{value: entry.Value})
		if err != nil {
			return nil, fmt.Errorf("BAML map entry %q: %w", entry.Key, err)
		}
		decoded[entry.Key] = mapValue
	}
	return decoded, nil
}

func MapDecoder[T any](decode func(Value) (T, error)) func(Value) (map[string]T, error) {
	return func(value Value) (map[string]T, error) { return DecodeMap(value, decode) }
}
