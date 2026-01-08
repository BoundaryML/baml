package serde

import (
	"reflect"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type TypeMap struct {
	allowDynamicUnion bool
	typeMap           map[string]reflect.Type
}

func (t TypeMap) GetType(name *cffi.CFFITypeName) (reflect.Type, bool) {
	ty_, ok := t.typeMap[name.Namespace.String()+"."+name.Name]
	return ty_, ok
}

func NewInternalTypeMap(typeMap map[string]reflect.Type) TypeMap {
	return TypeMap{
		allowDynamicUnion: false,
		typeMap:           typeMap,
	}
}

func NewExternalTypeMap(typeMap map[string]reflect.Type) TypeMap {
	return TypeMap{
		allowDynamicUnion: true,
		typeMap:           typeMap,
	}
}
