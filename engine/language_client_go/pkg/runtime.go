package baml

/*
#include <stdlib.h>
#include <stdint.h>

extern void trigger_callback(uint32_t id, int is_done, const int8_t *content, int length);
extern void error_callback(uint32_t id, int is_done, const int8_t *content, int length);
extern void on_tick_callback(uint32_t id);
*/
import "C"

import (
	"context"
	"encoding/json"
	"fmt"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go"
)

type BamlRuntime struct {
	runtime unsafe.Pointer
}

func NewClientRegistry() *ClientRegistry {
	return &ClientRegistry{}
}

func InvokeRuntimeCli(args []string) int {

	result, err := baml_go.InvokeRuntimeCli(args)
	if err != nil {
		fmt.Printf("Error invoking runtime cli: %v\n", err)
		return -1
	}
	return result
}

func init() {
	if err := baml_go.RegisterCallbacks(C.trigger_callback, C.error_callback, C.on_tick_callback); err != nil {
		panic(err)
	}
}

func CreateRuntime(
	root_path string,
	src_files map[string]string,
	env_vars map[string]string,
) (BamlRuntime, error) {

	src_files_json, err := json.Marshal(src_files)
	if err != nil {
		return BamlRuntime{}, err
	}

	env_vars_json, err := json.Marshal(env_vars)
	if err != nil {
		return BamlRuntime{}, err
	}

	runtime, err := baml_go.CreateBamlRuntime(
		root_path,
		string(src_files_json),
		string(env_vars_json),
	)
	if err != nil {
		return BamlRuntime{}, err
	}

	return BamlRuntime{runtime: runtime}, nil
}

func (r *BamlRuntime) CallFunction(ctx context.Context, functionName string, encoded_args []byte, onTick OnTickCallbackData) (*ResultCallback, error) {
	callback_id, callback := create_unique_id(ctx, onTick)

	// Channel to signal when the call is complete, so the goroutine can exit
	done := make(chan struct{})
	defer close(done)

	// Monitor context for early cancellation
	go func() {
		select {
		case <-ctx.Done():
			// Send cancellation to Rust immediately when context is done
			// This will trigger callback to send an error message
			baml_go.CancelFunctionCall(callback_id)
		case <-done:
			// Call completed, exit goroutine
		}
	}()

	err := baml_go.CallFunctionFromC(r.runtime, functionName, encoded_args, callback_id)
	if err != nil {
		close(callback)
		return nil, err
	}

	cb_result := <-callback
	return &cb_result, nil
}

func (r *BamlRuntime) CallFunctionStream(ctx context.Context, functionName string, encoded_args []byte, onTick OnTickCallbackData) (<-chan ResultCallback, error) {
	callback_id, callback := create_unique_id(ctx, onTick)

	err := baml_go.CallFunctionStreamFromC(r.runtime, functionName, encoded_args, callback_id)
	if err != nil {
		return nil, err
	}

	// Create a wrapper channel that forwards results and signals completion
	wrappedCallback := make(chan ResultCallback)

	go func() {
		defer close(wrappedCallback)
		for {
			select {
			case <-ctx.Done():
				// Send cancellation to Rust immediately when context is done
				baml_go.CancelFunctionCall(callback_id)
				return
			case result, ok := <-callback:
				if !ok {
					// Original channel closed, we're done
					return
				}
				wrappedCallback <- result
			}
		}
	}()

	return wrappedCallback, nil
}

func (r *BamlRuntime) BuildRequest(ctx context.Context, functionName string, encoded_args []byte) (HTTPRequest, error) {
	callback_id, callback := create_unique_id_for_object(ctx, r.runtime)

	// Channel to signal when the call is complete, so the goroutine can exit
	done := make(chan struct{})
	defer close(done)

	// Monitor context for early cancellation
	go func() {
		select {
		case <-ctx.Done():
			baml_go.CancelFunctionCall(callback_id)
		case <-done:
		}
	}()

	err := baml_go.BuildRequestFromC(r.runtime, functionName, encoded_args, callback_id)
	if err != nil {
		close(callback)
		return nil, err
	}

	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case result := <-callback:
		if result.Error != nil {
			return nil, result.Error
		}

		if result.HasData {
			httpReq, ok := result.Data.(HTTPRequest)
			if !ok {
				return nil, fmt.Errorf("unexpected type from build_request callback: %T", result.Data)
			}
			return httpReq, nil
		}

		return nil, fmt.Errorf("no data returned from build_request")
	}
}

func (r *BamlRuntime) CallFunctionParse(ctx context.Context, functionName string, encoded_args []byte) (any, error) {
	callback_id, callback := create_unique_id(ctx, nil)

	// Channel to signal when the call is complete, so the goroutine can exit
	done := make(chan struct{})
	defer close(done)

	// Monitor context for early cancellation
	go func() {
		select {
		case <-ctx.Done():
			// Send cancellation to Rust immediately when context is done
			baml_go.CancelFunctionCall(callback_id)
		case <-done:
			// Call completed, exit goroutine
		}
	}()

	err := baml_go.CallFunctionParseFromC(r.runtime, functionName, encoded_args, callback_id)
	if err != nil {
		return nil, err
	}

	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case result := <-callback:
		if result.Error != nil {
			return nil, result.Error
		}

		if result.HasData {
			return result.Data, nil
		} else {
			return result.StreamData, nil
		}
	}
}
