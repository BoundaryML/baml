package baml

/*
#include <stdlib.h>
#include <stdbool.h>
#include "./ext/baml.h"

extern void trigger_callback(uint32_t, bool, const int8_t *, int);
*/
import "C"

import (
	"context"
	"encoding/json"
	"sync"
	"unsafe"
)

type BamlRuntime struct {
	runtime unsafe.Pointer
}

var instance *BamlRuntime
var once sync.Once

func init() {
	C.register_callbacks((C.callback_fcn)(C.trigger_callback), (C.callback_fcn)(C.trigger_callback))
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

	src_files_c := C.CString(string(src_files_json))
	defer C.free(unsafe.Pointer(src_files_c))

	env_vars_c := C.CString(string(env_vars_json))
	defer C.free(unsafe.Pointer(env_vars_c))

	root_path_c := C.CString(root_path)
	defer C.free(unsafe.Pointer(root_path_c))

	runtime := C.create_baml_runtime(root_path_c, src_files_c, env_vars_c)
	return BamlRuntime{runtime: runtime}, nil
}

func (r *BamlRuntime) CallFunction(ctx context.Context, functionName string, arg_names []string, args ...any) (*ResultCallback, error) {
	functionNameC := C.CString(functionName)
	// defer C.free(unsafe.Pointer(functionNameC))

	callback_id, callback := create_unique_id(ctx)

	kwargsMap := make(map[string]any)
	for i, argName := range arg_names {
		kwargsMap[argName] = args[i]
	}
	kwargs, err := json.Marshal(kwargsMap)
	if err != nil {
		return nil, err
	}
	kwargsC := C.CString(string(kwargs))
	// defer C.free(unsafe.Pointer(kwargsC))

	return_channel := make(chan ResultCallback)
	go func() {
		for {
			select {
			case <-ctx.Done():
				close(return_channel)
				return
			case result := <-callback:
				// TODO: Handle the result
				// error handling, type checking, etc.
				return_channel <- result
			}
		}
	}()
	C.call_function_from_c(r.runtime, functionNameC, kwargsC, callback_id)

	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case result := <-return_channel:
		return &result, nil
	}
}

func (r *BamlRuntime) CallFunctionStream(ctx context.Context, functionName string, arg_names []string, args ...any) (<-chan ResultCallback, error) {
	functionNameC := C.CString(functionName)
	// defer C.free(unsafe.Pointer(functionNameC))

	callback_id, callback := create_unique_id(ctx)
	kwargsMap := make(map[string]any)
	for i, argName := range arg_names {
		kwargsMap[argName] = args[i]
	}
	kwargs, err := json.Marshal(kwargsMap)
	if err != nil {
		return nil, err
	}
	kwargsC := C.CString(string(kwargs))
	// defer C.free(unsafe.Pointer(kwargsC))

	return_channel := make(chan ResultCallback)
	go func() {
		for {
			select {
			case <-ctx.Done():
				close(return_channel)
				return
			case result, ok := <-callback:
				if !ok {
					close(return_channel)
					return
				}
				// TODO: Handle the result
				// error handling, type checking, etc.
				return_channel <- result
			}
		}
	}()

	C.call_function_stream_from_c(r.runtime, functionNameC, kwargsC, callback_id)

	return return_channel, nil
}
