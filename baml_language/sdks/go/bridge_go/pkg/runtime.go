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

// FunctionOperation selects a semantic projection of an authored BAML
// function. Direct is zero so callers using an older encoder keep the existing
// behavior when the protobuf field is absent.
type FunctionOperation int32

const (
	FunctionOperationDirect FunctionOperation = iota
	FunctionOperationSpec
	FunctionOperationStream
)

func (operation FunctionOperation) valid() bool {
	return operation >= FunctionOperationDirect && operation <= FunctionOperationStream
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
	return rt.CallFunctionOperation(ctx, name, FunctionOperationDirect, args)
}

// CallFunctionOperation calls a semantic projection on the original authored
// function name. The operation is carried separately on the wire; no `$...`
// companion name is constructed.
func (rt *BamlRuntime) CallFunctionOperation(ctx context.Context, name string, operation FunctionOperation, args map[string]any) (any, error) {
	callbackID, ch := createUniqueID()

	encodedArgs, err := encodeCallArgsWithOperation(args, name, uint64(callbackID), operation)
	if err != nil {
		deleteCallback(callbackID)
		return nil, fmt.Errorf("encoding args: %w", err)
	}

	cffi.CallFunction(encodedArgs, callbackID)

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
