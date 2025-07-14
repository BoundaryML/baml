package raw_objects

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type llmStreamCall struct {
	*llmCall
}

func newLLMStreamCall(ptr int64) LLMStreamCall {
	return &llmStreamCall{&llmCall{&rawObject{ptr: ptr}}}
}

func (l *llmStreamCall) objectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_LLM_CALL
}

func (l *llmStreamCall) SSEChunks() ([]SSEResponse, error) {
	result, err := callMethod(l, "sse_chunks", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get SSE chunks: %w", err)
	}

	chunks, ok := result.([]SSEResponse)
	if !ok {
		return nil, fmt.Errorf("unexpected type for SSE chunks: %T", result)
	}

	return chunks, nil
}
