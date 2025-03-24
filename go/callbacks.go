package baml

/*
#cgo LDFLAGS: ./ext/libbaml.dylib -ldl
#include <stdlib.h>
#include <stdbool.h>
#include <stdint.h>
#include "./ext/baml.h"
*/
import "C"

import (
	"context"
	"fmt"
	"math/rand"
	"sync"
	"unsafe"

	"github.com/boundaryml/baml/go/CFFI"
	flatbuffers "github.com/google/flatbuffers/go"
)

type ResultCallback struct {
	data CFFI.CFFIValueHolder // JSON string
}

func (r *ResultCallback) Raw() string {
	return fmt.Sprintf("%s: %s", r.data.ValueType().String(), len(r.data.Table().Bytes))
}

type CallbackData struct {
	channel chan ResultCallback
	ctx     context.Context
}

// Map to store callbacks by ID
var (
	dynamicCallbacks = make(map[uint32]CallbackData)
	callbackMutex    sync.RWMutex
)

//export trigger_callback
func trigger_callback(id C.uint32_t, isDone C.bool, content *C.int8_t, length C.int) {
	callbackMutex.RLock()
	id_uint := uint32(id)
	callback, exists := dynamicCallbacks[id_uint]
	callbackMutex.RUnlock()

	if exists {
		content_bytes := C.GoBytes(unsafe.Pointer(content), length)

		parsed_data := CFFI.CFFIValueHolder{}
		flatbuffers.GetRootAs(content_bytes, 0, &parsed_data)

		my_string := fmt.Sprintf("Length: %d, Type: %s", length, parsed_data.ValueType().String())
		fmt.Println("My string: ", my_string)
		force_close := false

		select {
		case <-callback.ctx.Done():
			force_close = true
			// TODO: Somehow tell rust to die
			break
		case callback.channel <- ResultCallback{data: parsed_data}:
			fmt.Println("Sending data to channel")
			break
		}

		if bool(isDone) || force_close {
			fmt.Println("Closing channel")
			close(callback.channel)
			callbackMutex.Lock()
			defer callbackMutex.Unlock()
			delete(dynamicCallbacks, id_uint)
		}
	}
}

func create_unique_id(ctx context.Context) (C.uint32_t, chan ResultCallback) {
	callbackMutex.Lock()
	defer callbackMutex.Unlock()
	id := uint32(rand.Intn(1000000))
	for _, exists := dynamicCallbacks[id]; exists; {
		id = uint32(rand.Intn(1000000))
	}
	dynamicCallbacks[id] = CallbackData{channel: make(chan ResultCallback), ctx: ctx}
	return C.uint32_t(id), dynamicCallbacks[id].channel
}
