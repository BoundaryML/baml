package raw_objects

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type httpRequest struct {
	*rawObject
}

func newHttpRequest(ptr int64) HttpRequest {
	return &httpRequest{&rawObject{ptr: ptr}}
}

func (h *httpRequest) objectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_HTTP_REQUEST
}

func (h *httpRequest) pointer() int64 {
	return h.rawObject.pointer()
}

func (h *httpRequest) URL() (string, error) {
	result, err := callMethod(h, "url", nil)
	if err != nil {
		return "", fmt.Errorf("failed to get URL: %w", err)
	}

	url, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type for URL: %T", result)
	}

	return url, nil
}

func (h *httpRequest) Method() (string, error) {
	result, err := callMethod(h, "method", nil)
	if err != nil {
		return "", fmt.Errorf("failed to get method: %w", err)
	}

	method, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type for method: %T", result)
	}

	return method, nil
}

func (h *httpRequest) Headers() (map[string]string, error) {
	result, err := callMethod(h, "headers", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get headers: %w", err)
	}

	headers, ok := result.(map[string]string)
	if !ok {
		return nil, fmt.Errorf("unexpected type for headers: %T", result)
	}

	return headers, nil
}

func (h *httpRequest) Body() (HTTPBody, error) {
	result, err := callMethod(h, "body", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get body: %w", err)
	}

	body, ok := result.(HTTPBody)
	if !ok {
		return nil, fmt.Errorf("unexpected type for body: %T", result)
	}

	return body, nil
}
