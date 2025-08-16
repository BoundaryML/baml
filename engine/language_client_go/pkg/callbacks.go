package baml

/*
#include <stdlib.h>
#include <stdint.h>
*/
import "C"

import (
	"context"
	"math/rand"
	"sync"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/serde"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
	"google.golang.org/protobuf/proto"
)

type BamlError struct {
	Message string
}

func (e BamlError) Error() string {
	return e.Message
}

type BamlClientError struct {
	BamlError
}

type BamlClientHttpError struct {
	BamlClientError
}

type ResultCallback struct {
	Error         error
	HasStreamData bool
	HasData       bool
	StreamData    any
	Data          any
}

type CallbackData struct {
	channel chan ResultCallback
	ctx     context.Context
	onTick  OnTickCallbackData
}

type OnTickCallbackData interface {
	Collector() Collector
	OnTick() TickCallback
}

// Map to store callbacks by ID
var (
	dynamicCallbacks = make(map[uint32]CallbackData)
	callbackMutex    sync.RWMutex
	typeMap          serde.TypeMap
)

func SetTypeMap(t serde.TypeMap) {
	typeMap = t
}

//export on_tick_callback
func on_tick_callback(id C.uint32_t) {
	callbackMutex.RLock()
	id_uint := uint32(id)
	callback, exists := dynamicCallbacks[id_uint]
	callbackMutex.RUnlock()

	if exists {
		data := callback.onTick
		if data != nil {
			last, err := data.Collector().Last()
			if err != nil {
				return
			}
			data.OnTick()(callback.ctx, TickReason_Unknown, last)
		}
	}
}

//export error_callback
func error_callback(id C.uint32_t, isDone C.int, content *C.int8_t, length C.int) {
	callbackMutex.RLock()
	id_uint := uint32(id)
	callback, exists := dynamicCallbacks[id_uint]
	callbackMutex.RUnlock()

	if exists {
		content_bytes := C.GoBytes(unsafe.Pointer(content), length)

		// Parse the content as a string
		content_str := string(content_bytes)

		// TODO: cast to the right error type
		err := BamlError{Message: content_str}

		// Send the error to the callback
		callback.channel <- ResultCallback{Error: err}

		close(callback.channel)
		callbackMutex.Lock()
		defer callbackMutex.Unlock()
		delete(dynamicCallbacks, id_uint)
	}
}

//export trigger_callback
func trigger_callback(id C.uint32_t, isDone C.int, content *C.int8_t, length C.int) {
	callbackMutex.RLock()
	id_uint := uint32(id)
	callback, exists := dynamicCallbacks[id_uint]
	callbackMutex.RUnlock()

	if exists {
		content_bytes := C.GoBytes(unsafe.Pointer(content), length)

		var content_holder cffi.CFFIValueHolder
		err := proto.Unmarshal(content_bytes, &content_holder)
		if err != nil {
			callback.channel <- ResultCallback{Error: err}
			close(callback.channel)
			callbackMutex.Lock()
			defer callbackMutex.Unlock()
			delete(dynamicCallbacks, id_uint)
			return
		}

		decoded_data := serde.Decode(&content_holder, typeMap).Interface()

		var res ResultCallback
		if isDone == 1 {
			res = ResultCallback{HasData: true, Data: decoded_data}
		} else {
			res = ResultCallback{HasStreamData: true, StreamData: decoded_data}
		}

		force_close := false

		select {
		case <-callback.ctx.Done():
			force_close = true
			callback.channel <- ResultCallback{Error: callback.ctx.Err()}
			// Cancellation is now handled early in runtime.go
			break
		case callback.channel <- res:
			break
		}

		if isDone == 1 || force_close {
			close(callback.channel)
			callbackMutex.Lock()
			defer callbackMutex.Unlock()
			delete(dynamicCallbacks, id_uint)
		}
	}
}

func create_unique_id(ctx context.Context, onTick OnTickCallbackData) (uint32, chan ResultCallback) {
	callbackMutex.Lock()
	defer callbackMutex.Unlock()
	id := uint32(rand.Intn(1000000))
	for _, exists := dynamicCallbacks[id]; exists; {
		id = uint32(rand.Intn(1000000))
	}
	dynamicCallbacks[id] = CallbackData{channel: make(chan ResultCallback), ctx: ctx, onTick: onTick}
	return id, dynamicCallbacks[id].channel
}
