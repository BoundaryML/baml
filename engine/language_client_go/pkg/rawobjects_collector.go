package baml

import (
	"fmt"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type collector struct {
	*raw_objects.RawObject
}

func newCollector(ptr int64, rt unsafe.Pointer) Collector {
	return &collector{raw_objects.FromPointer(ptr, rt)}
}

func (c *collector) ObjectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_COLLECTOR
}

func (c *collector) pointer() int64 {
	return c.RawObject.Pointer()
}

func (c *collector) Usage() (Usage, error) {
	result, err := raw_objects.CallMethod(c, "usage", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get usage: %w", err)
	}

	usage, ok := result.(Usage)
	if !ok {
		return nil, fmt.Errorf("unexpected type for usage: %T", result)
	}

	return usage, nil
}

func (c *collector) Name() (string, error) {
	result, err := raw_objects.CallMethod(c, "name", nil)
	if err != nil {
		return "", fmt.Errorf("failed to get name: %w", err)
	}

	name, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type for name: %T", result)
	}

	return name, nil
}

func (c *collector) Logs() ([]FunctionLog, error) {
	result, err := raw_objects.CallMethod(c, "logs", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get logs: %w", err)
	}

	logs, ok := result.([]raw_objects.RawPointer)
	if !ok {
		return nil, fmt.Errorf("unexpected type for logs: %T", result)
	}

	functionLogs := make([]FunctionLog, len(logs))
	for i, log := range logs {
		cast, ok := log.(FunctionLog)
		if !ok {
			return nil, fmt.Errorf("unexpected type in logs: %T", log)
		}
		functionLogs[i] = cast
	}

	return functionLogs, nil
}

func (c *collector) Last() (FunctionLog, error) {
	result, err := raw_objects.CallMethod(c, "last", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get last log: %w", err)
	}

	if as_nil, ok := result.(*interface{}); ok && as_nil == nil {
		return nil, nil // No last log available
	}

	log, ok := result.(FunctionLog)
	if !ok {
		return nil, fmt.Errorf("unexpected type for last log: %T %v", result, result)
	}

	return log, nil
}

func (c *collector) Id(functionId string) (FunctionLog, error) {
	result, err := raw_objects.CallMethod(c, "id", map[string]any{
		"id": functionId,
	})
	if err != nil {
		return nil, fmt.Errorf("failed to get log by id: %w", err)
	}

	log, ok := result.(FunctionLog)
	if !ok {
		return nil, fmt.Errorf("unexpected type for log by id: %T", result)
	}

	return log, nil
}

func (c *collector) Clear() (int64, error) {
	result, err := raw_objects.CallMethod(c, "clear", nil)
	if err != nil {
		return 0, fmt.Errorf("failed to clear: %w", err)
	}

	count, ok := result.(int64)
	if !ok {
		return 0, fmt.Errorf("unexpected type for clear result: %T", result)
	}

	return count, nil
}
