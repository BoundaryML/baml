package baml

import (
	"fmt"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type llmStreamCall struct {
	*llmCall
}

func newLLMStreamCall(ptr int64, rt unsafe.Pointer) LLMStreamCall {
	return &llmStreamCall{&llmCall{raw_objects.FromPointer(ptr, rt)}}
}

func (l *llmStreamCall) ObjectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_LLM_STREAM_CALL
}

func (l *llmStreamCall) SSEChunks() ([]SSEResponse, error) {
	result, err := raw_objects.CallMethod(l, "sse_chunks", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get SSE chunks: %w", err)
	}

	casted, ok := result.([]raw_objects.RawPointer)
	if !ok {
		return nil, fmt.Errorf("unexpected type for SSE chunks: %T", result)
	}

	chunks := make([]SSEResponse, len(casted))
	for i, chunk := range casted {
		chunks[i] = chunk.(SSEResponse)
	}

	return chunks, nil
}
