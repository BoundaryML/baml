package baml_go

import (
	"reflect"
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
)

func TestNilContainersEncodeAsPresentEmptyContainers(t *testing.T) {
	list := List[string](nil, String)
	listValue, ok := list.value.Value.(*cffi.InboundValue_ListValue)
	if !ok || listValue.ListValue == nil || len(listValue.ListValue.Values) != 0 {
		t.Fatalf("nil slice encoded as %#v, want present empty list", list.value)
	}

	mapInput := Map[string](nil, String)
	mapValue, ok := mapInput.value.Value.(*cffi.InboundValue_MapValue)
	if !ok || mapValue.MapValue == nil || len(mapValue.MapValue.Entries) != 0 {
		t.Fatalf("nil map encoded as %#v, want present empty map", mapInput.value)
	}
}

func TestMapEncodingIsDeterministic(t *testing.T) {
	input := Map(map[string]string{"z": "last", "a": "first"}, String)
	encoded := input.value.GetMapValue().Entries
	if len(encoded) != 2 || encoded[0].GetStringKey() != "a" || encoded[1].GetStringKey() != "z" {
		t.Fatalf("map keys encoded in order %#v", encoded)
	}
}

func TestDecodeContainersRejectMalformedWireValues(t *testing.T) {
	list := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{
		ListValue: &cffi.BamlValueList{Items: []*cffi.BamlOutboundValue{nil}},
	}}}
	if _, err := DecodeList(list, Value.String); err == nil {
		t.Fatal("list with empty item unexpectedly decoded")
	}

	stringValue := func(value string) *cffi.BamlOutboundValue {
		return &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: value}}
	}
	mapValue := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_MapValue{
		MapValue: &cffi.BamlValueMap{Entries: []*cffi.BamlOutboundMapEntry{
			{Key: "duplicate", Value: stringValue("first")},
			{Key: "duplicate", Value: stringValue("second")},
		}},
	}}}
	if _, err := DecodeMap(mapValue, Value.String); err == nil {
		t.Fatal("map with duplicate key unexpectedly decoded")
	}
}

func TestContainerDecodersPreserveValuesAndReturnPresentEmptyContainers(t *testing.T) {
	emptyList := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{
		ListValue: &cffi.BamlValueList{},
	}}}
	list, err := DecodeList(emptyList, Value.String)
	if err != nil {
		t.Fatal(err)
	}
	if list == nil || len(list) != 0 {
		t.Fatalf("decoded empty list as %#v", list)
	}

	emptyMap := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_MapValue{
		MapValue: &cffi.BamlValueMap{},
	}}}
	mapValue, err := DecodeMap(emptyMap, Value.String)
	if err != nil {
		t.Fatal(err)
	}
	if mapValue == nil || !reflect.DeepEqual(mapValue, map[string]string{}) {
		t.Fatalf("decoded empty map as %#v", mapValue)
	}
}
