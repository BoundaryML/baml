package raw_objects

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/serde"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type collector struct {
	*rawObject
}

func NewCollector(name string) (Collector, error) {
	kwargs, err := serde.EncodeMapEntries(map[string]any{
		"name": name,
	}, "collector constructor args")
	if err != nil {
		return nil, fmt.Errorf("failed to encode kwargs: %w", err)
	}

	ptr, err := newRawObject(cffi.CFFIObjectType_OBJECT_COLLECTOR, kwargs)
	if err != nil {
		return nil, fmt.Errorf("failed to create collector: %w", err)
	}

	as_collector, ok := ptr.(*collector)
	if !ok {
		return nil, fmt.Errorf("unexpected type for collector creation: %T", ptr)
	}

	return as_collector, nil
}

func newCollector(ptr int64) Collector {
	return &collector{&rawObject{ptr: ptr}}
}

func (c *collector) objectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_COLLECTOR
}

func (c *collector) pointer() int64 {
	return c.rawObject.pointer()
}

func (c *collector) Usage() (Usage, error) {
	result, err := callMethod(c, "usage", nil)
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
	result, err := callMethod(c, "name", nil)
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
	result, err := callMethod(c, "logs", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get logs: %w", err)
	}

	logs, ok := result.([]rawPointer)
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
	result, err := callMethod(c, "last", nil)
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
	result, err := callMethod(c, "id", map[string]any{
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

func (c *collector) Clear() error {
	_, err := callMethod(c, "clear", nil)
	if err != nil {
		return fmt.Errorf("failed to clear: %w", err)
	}

	return nil
}
