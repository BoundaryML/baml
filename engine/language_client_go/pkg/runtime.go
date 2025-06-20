package baml

/*
#include <stdlib.h>
#include <stdint.h>

extern void trigger_callback(uint32_t id, int is_done, const int8_t *content, int length);
extern void error_callback(uint32_t id, int is_done, const int8_t *content, int length);
*/
import "C"

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"sync"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go"
)

type BamlRuntime struct {
	runtime unsafe.Pointer
}

type BamlFunctionArguments struct {
	Kwargs         map[string]any
	ClientRegistry *ClientRegistry
	Env            map[string]string
	Collectors     []Collector
}

type ClientRegistry struct {
	primary *string
	clients clientRegistryMap
}

type clientProperty struct {
	provider    string
	retryPolicy string
	options     map[string]any
}

type clientRegistryMap map[string]clientProperty

func NewClientRegistry() *ClientRegistry {
	return &ClientRegistry{
		primary: nil,
		clients: clientRegistryMap{},
	}
}

func (c *ClientRegistry) AddLlmClient(name string, provider string, options map[string]any) {
	c.clients[name] = clientProperty{
		provider: provider,
		options:  options,
	}
}

func (c *ClientRegistry) SetPrimaryClient(name string) {
	c.primary = &name
}

var instance *BamlRuntime
var once sync.Once

func InvokeRuntimeCli(args []string) int {

	result, err := baml_go.InvokeRuntimeCli(args)
	if err != nil {
		fmt.Printf("Error invoking runtime cli: %v\n", err)
		return -1
	}
	return result
}

func init() {
	if err := baml_go.RegisterCallbacks(C.trigger_callback, C.error_callback); err != nil {
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

func (r *BamlRuntime) CallFunction(ctx context.Context, functionName string, encoded_args []byte) (*ResultCallback, error) {
	callback_id, callback := create_unique_id(ctx)
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

	result, err := baml_go.CallFunctionFromC(r.runtime, functionName, encoded_args, callback_id)
	if err != nil {
		close(return_channel)
		return nil, err
	}

	if result != nil {
		result_str := (*string)(result)
		close(return_channel)
		return nil, errors.New(*result_str)
	}

	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case result := <-return_channel:
		return &result, nil
	}
}

func (r *BamlRuntime) CallFunctionStream(ctx context.Context, functionName string, encoded_args []byte) (<-chan ResultCallback, error) {
	callback_id, callback := create_unique_id(ctx)

	result, err := baml_go.CallFunctionStreamFromC(r.runtime, functionName, encoded_args, callback_id)
	if err != nil {
		return nil, err
	}

	if result != nil {
		result_str := (*string)(result)
		return nil, errors.New(*result_str)
	}

	return callback, nil
}
