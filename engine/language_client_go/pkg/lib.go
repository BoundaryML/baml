package baml

import (
	"reflect"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/serde"
	"github.com/boundaryml/baml/engine/language_client_go/baml_go/shared"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

func EncodeClass(name string, fields map[string]any, dynamicFields *map[string]any) (*cffi.HostValue, error) {
	return serde.EncodeClass(name, fields, dynamicFields)
}

func EncodeEnum(name string, value string, is_dynamic bool) (*cffi.HostValue, error) {
	return serde.EncodeEnum(name, value, is_dynamic)
}

func EncodeValue(value any) (*cffi.HostValue, error) {
	return serde.EncodeValue(value)
}

func Decode(holder *cffi.CFFIValueHolder) reflect.Value {
	raw_decoded_data, _ := serde.Decode(holder, typeMap)
	return raw_decoded_data
}

func DecodeToValue(holder *cffi.CFFIValueHolder) any {
	raw_decoded_data, goType := serde.Decode(holder, typeMap)

	if !raw_decoded_data.IsValid() {
		return nil
	}

	// If int, bool, float, string, return the value directly
	if goType == reflect.TypeOf(int64(0)) {
		return raw_decoded_data.Int()
	}
	if goType == reflect.TypeOf(float64(0)) {
		return raw_decoded_data.Float()
	}
	if goType == reflect.TypeOf(false) {
		return raw_decoded_data.Bool()
	}
	return raw_decoded_data.Interface()
}

func BAMLTESTINGONLY_InternalEncode(value any) (*cffi.HostValue, error) {
	return serde.EncodeValue(value)
}

type TypeMap = serde.TypeMap
type Checked[T any] = shared.Checked[T]
type StreamState[T any] = shared.StreamState[T]
type StreamingStateType = shared.StreamStateType

const (
	StreamStatePending    = shared.StreamStatePending
	StreamStateIncomplete = shared.StreamStateIncomplete
	StreamStateComplete   = shared.StreamStateComplete
)
