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
	return serde.Decode(holder, typeMap)
}

func DecodeStreamingState[T any](holder *cffi.CFFIValueHolder, decodeFunc func(inner *cffi.CFFIValueHolder) T) shared.StreamState[T] {
	return serde.DecodeStreamingState(holder, decodeFunc)
}

func DecodeChecked[T any](holder *cffi.CFFIValueHolder, decodeFunc func(inner *cffi.CFFIValueHolder) T) shared.Checked[T] {
	return serde.DecodeChecked(holder, decodeFunc)
}

func CastChecked[T any](value any, castFunc func(inner any) T) shared.Checked[T] {
	return serde.CastChecked(value, castFunc)
}

func CastStreamState[T any](value any, castFunc func(inner any) T) shared.StreamState[T] {
	return serde.CastStreamState(value, castFunc)
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
