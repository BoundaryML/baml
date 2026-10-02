package pkg

import (
	pb "bridge_go/cffi/proto/baml_bridge/cffi/v1"
	"context"
	"encoding/json"
	"fmt"
	"math"
	"time"
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
	callbackID, ch := createUniqueID()

	callID := cffi.NewFunctionCall()
	if callID == 0 {
		deleteCallback(callbackID)
		return nil, fmt.Errorf("runtime call allocation failed")
	}
	controls := &pb.InvocationOptions{HostEnvironment: callID}
	if frame, ok := ctx.Value(invocationStateContextKey{}).(*callbackInvocationFrame); ok {
		controls.InheritedState = frame.state
	}
	if deadline, ok := ctx.Deadline(); ok {
		now, err := cffi.InvocationClockNs(callID)
		if err != nil {
			cffi.ReleaseFunctionCall(callID)
			deleteCallback(callbackID)
			return nil, err
		}
		remaining := time.Until(deadline)
		absolute := now
		if remaining > 0 {
			if uint64(remaining) > math.MaxUint64-now {
				cffi.ReleaseFunctionCall(callID)
				deleteCallback(callbackID)
				return nil, fmt.Errorf("invocation deadline overflow")
			}
			absolute += uint64(remaining)
		}
		controls.DeadlineNs = &absolute
	}
	// Resolve controls before encoding application values: failures here must
	// not strand the owners minted by the value encoder.
	encodedArgs, err := encodeCallArgs(args, name, callID, controls)
	if err != nil {
		cffi.ReleaseFunctionCall(callID)
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
		cffi.CancelFunctionCall(callID)
		deleteCallback(callbackID)
		return nil, ctx.Err()
	}
}
