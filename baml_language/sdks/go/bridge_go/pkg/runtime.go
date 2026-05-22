package pkg

import (
	"context"
	"encoding/json"
	"fmt"
	"unsafe"

	"bridge_go/cffi"
)

// BamlRuntime wraps the BAML engine runtime.
type BamlRuntime struct {
	ptr unsafe.Pointer
}

// NewRuntime creates a BAML runtime from virtual filesystem source files.
func NewRuntime(rootPath string, files map[string]string) (*BamlRuntime, error) {
	if files == nil {
		files = map[string]string{}
	}
	filesJSON, err := json.Marshal(files)
	if err != nil {
		return nil, fmt.Errorf("marshaling source files: %w", err)
	}
	ptr, err := cffi.CreateBamlRuntime(rootPath, string(filesJSON))
	if err != nil {
		return nil, err
	}
	return &BamlRuntime{ptr: ptr}, nil
}

// Version returns the BAML engine version string.
func Version() string {
	return cffi.Version()
}

// CallFunction calls a BAML function asynchronously and returns the decoded Go result.
func (rt *BamlRuntime) CallFunction(ctx context.Context, name string, args map[string]any) (any, error) {
	encodedArgs, err := encodeCallArgs(args)
	if err != nil {
		return nil, fmt.Errorf("encoding args: %w", err)
	}

	callbackID, ch := createUniqueID()

	cffi.CallFunction(name, encodedArgs, callbackID)

	select {
	case result := <-ch:
		if result.Error != nil {
			return nil, result.Error
		}
		return result.Data, nil
	case <-ctx.Done():
		cffi.CancelFunctionCall(callbackID)
		deleteCallback(callbackID)
		return nil, ctx.Err()
	}
}
