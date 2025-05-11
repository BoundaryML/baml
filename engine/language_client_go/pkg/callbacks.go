package baml

/*
#include <stdlib.h>
#include <stdint.h>
*/
import "C"

import (
	"context"
	"fmt"
	"math/rand"
	"sync"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
	flatbuffers "github.com/google/flatbuffers/go"
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
	StreamData    *any
	Data          *any
}

type CallbackData struct {
	channel chan ResultCallback
	ctx     context.Context
}

// Map to store callbacks by ID
var (
	dynamicCallbacks = make(map[uint32]CallbackData)
	callbackMutex    sync.RWMutex
	typeMap          TypeMap
)

func SetTypeMap(t TypeMap) {
	typeMap = t
}

//export error_callback
func error_callback(id C.uint32_t, isDone C.int, content *C.int8_t, length C.int) {
	fmt.Println("Error callback")
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

		parsed_data := cffi.CFFIValueHolder{}
		flatbuffers.GetRootAs(content_bytes, 0, &parsed_data)
		decoded_data := Decode(&parsed_data)

		var res ResultCallback
		if isDone == 1 {
			res = ResultCallback{HasData: true, Data: &decoded_data}
		} else {
			res = ResultCallback{HasStreamData: true, StreamData: &decoded_data}
		}

		force_close := false

		select {
		case <-callback.ctx.Done():
			force_close = true
			// TODO: Somehow tell rust to die
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

func create_unique_id(ctx context.Context) (uint32, chan ResultCallback) {
	callbackMutex.Lock()
	defer callbackMutex.Unlock()
	id := uint32(rand.Intn(1000000))
	for _, exists := dynamicCallbacks[id]; exists; {
		id = uint32(rand.Intn(1000000))
	}
	dynamicCallbacks[id] = CallbackData{channel: make(chan ResultCallback), ctx: ctx}
	return id, dynamicCallbacks[id].channel
}
