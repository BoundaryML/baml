package baml

import (
	"reflect"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/serde"
	"github.com/boundaryml/baml/engine/language_client_go/baml_go/shared"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

func EncodeClass(name func() *cffi.CFFITypeName, fields map[string]any, dynamicFields *map[string]any) (*cffi.CFFIValueHolder, error) {
	return serde.EncodeClass(name, fields, dynamicFields)
}

func EncodeEnum(name func() *cffi.CFFITypeName, value string, is_dynamic bool) (*cffi.CFFIValueHolder, error) {
	return serde.EncodeEnum(name, value, is_dynamic)
}

func EncodeUnion(name func() *cffi.CFFITypeName, variant string, value any) (*cffi.CFFIValueHolder, error) {
	return serde.EncodeUnion(name, variant, value)
}

func Decode(holder *cffi.CFFIValueHolder) reflect.Value {
	return serde.Decode(holder, typeMap)
}

func DecodeStreamingState[T any](holder *cffi.CFFIValueHolder, decodeFunc func(inner *cffi.CFFIValueHolder) T) shared.StreamState[T] {
	return serde.DecodeStreamingState(holder, decodeFunc)
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
