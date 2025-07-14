package raw_objects

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type timing struct {
	*rawObject
}

func newTiming(ptr int64) Timing {
	return &timing{&rawObject{ptr: ptr}}
}

func (t *timing) objectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_TIMING
}

func (t *timing) pointer() int64 {
	return t.rawObject.pointer()
}

func (t *timing) StartTimeUTCMs() (int64, error) {
	result, err := callMethod(t, "start_time_utc_ms", nil)
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
	result, err := callMethod(t, "duration_ms", nil)
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
