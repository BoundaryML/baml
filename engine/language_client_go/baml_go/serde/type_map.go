package serde

import (
	"reflect"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type TypeMap map[string]reflect.Type

func (t TypeMap) GetType(name *cffi.CFFITypeName) (reflect.Type, bool) {
	ty_, ok := t[name.Namespace.String()+"."+name.Name]
	return ty_, ok
}
