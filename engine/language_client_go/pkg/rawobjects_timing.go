package baml

import (
	"fmt"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type timing struct {
	*raw_objects.RawObject
}

func newTiming(ptr int64, rt unsafe.Pointer) Timing {
	return &timing{raw_objects.FromPointer(ptr, rt)}
}

func (t *timing) ObjectType() cffi.BamlObjectType {
	return cffi.BamlObjectType_OBJECT_TIMING
}

func (t *timing) pointer() int64 {
	return t.RawObject.Pointer()
}

func (t *timing) StartTimeUTCMs() (int64, error) {
	result, err := raw_objects.CallMethod(t, "start_time_utc_ms", nil)
	if err != nil {
		return 0, fmt.Errorf("failed to get start time: %w", err)
	}

	startTime, ok := result.(int64)
	if !ok {
		return 0, fmt.Errorf("unexpected type for start time: %T", result)
	}

	return startTime, nil
}

func (t *timing) DurationMs() (*int64, error) {
	result, err := raw_objects.CallMethod(t, "duration_ms", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get duration: %w", err)
	}

	if result == nil {
		return nil, nil
	}

	duration, ok := result.(int64)
	if !ok {
		return nil, fmt.Errorf("unexpected type for duration: %T", result)
	}

	return &duration, nil
}
