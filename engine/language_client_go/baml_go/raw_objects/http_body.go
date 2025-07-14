package raw_objects

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type httpBody struct {
	*rawObject
}

func newHTTPBody(ptr int64) HTTPBody {
	return &httpBody{&rawObject{ptr: ptr}}
}

func (h *httpBody) objectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_HTTP_BODY
}

func (h *httpBody) pointer() int64 {
	return h.rawObject.pointer()
}

func (h *httpBody) Text() (string, error) {
	result, err := callMethod(h, "text", nil)
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
	result, err := callMethod(h, "json", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get JSON: %w", err)
	}

	return result, nil
}
