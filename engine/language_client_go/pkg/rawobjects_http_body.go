package baml

import (
	"fmt"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type httpBody struct {
	*raw_objects.RawObject
}

func newHTTPBody(ptr int64, rt unsafe.Pointer) HTTPBody {
	return &httpBody{raw_objects.FromPointer(ptr, rt)}
}

func (h *httpBody) ObjectType() cffi.BamlObjectType {
	return cffi.BamlObjectType_OBJECT_HTTP_BODY
}

func (h *httpBody) pointer() int64 {
	return h.RawObject.Pointer()
}

func (h *httpBody) Text() (string, error) {
	result, err := raw_objects.CallMethod(h, "text", nil)
	if err != nil {
		return "", fmt.Errorf("failed to get text: %w", err)
	}

	text, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type for text: %T", result)
	}

	return text, nil
}

func (h *httpBody) JSON() (any, error) {
	result, err := raw_objects.CallMethod(h, "json", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get JSON: %w", err)
	}

	return result, nil
}
