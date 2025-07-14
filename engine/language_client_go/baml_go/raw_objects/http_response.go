package raw_objects

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type httpResponse struct {
	*rawObject
}

func newHttpResponse(ptr int64) HttpResponse {
	return &httpResponse{&rawObject{ptr: ptr}}
}

func (h *httpResponse) objectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_HTTP_RESPONSE
}

func (h *httpResponse) pointer() int64 {
	return h.rawObject.pointer()
}

func (h *httpResponse) Status() (int, error) {
	result, err := callMethod(h, "status", nil)
	if err != nil {
		return 0, fmt.Errorf("failed to get status: %w", err)
	}

	status, ok := result.(int)
	if !ok {
		return 0, fmt.Errorf("unexpected type for status: %T", result)
	}

	return status, nil
}

func (h *httpResponse) Headers() (map[string]string, error) {
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

func (h *httpResponse) Body() (HTTPBody, error) {
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
