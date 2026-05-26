package pkg

/*
#include <stdint.h>
*/
import "C"
import (
	"sync"
	"sync/atomic"
	"unsafe"
)

// ResultCallback is sent on the channel when Rust completes a function call.
type ResultCallback struct {
	Error error
	Data  any
}

type callbackData struct {
	channel chan ResultCallback
}

var (
	dynamicCallbacks = make(map[uint32]callbackData)
	callbackMutex    sync.RWMutex
	nextCallbackID   atomic.Uint32
)

func createUniqueID() (uint32, chan ResultCallback) {
	id := nextCallbackID.Add(1)
	ch := make(chan ResultCallback, 64)
	callbackMutex.Lock()
	dynamicCallbacks[id] = callbackData{channel: ch}
	callbackMutex.Unlock()
	return id, ch
}

func deleteCallback(id uint32) {
	callbackMutex.Lock()
	delete(dynamicCallbacks, id)
	callbackMutex.Unlock()
}

func safeSend(ch chan ResultCallback, res ResultCallback) {
	defer func() { recover() }()
	ch <- res
}

func safeClose(ch chan ResultCallback) {
	defer func() { recover() }()
	close(ch)
}

// baml_callback is the single unified callback from Rust.
// is_error=0: content is protobuf-encoded BamlOutboundValue (success)
// is_error=1: content is UTF-8 error string
//
//export baml_callback
func baml_callback(id C.uint32_t, isError C.int32_t, content *C.int8_t, length C.size_t) {
	callbackMutex.RLock()
	cb, exists := dynamicCallbacks[uint32(id)]
	callbackMutex.RUnlock()
	if !exists {
		return
	}

	contentBytes := C.GoBytes(unsafe.Pointer(content), C.int(length))

	if isError != 0 {
		errMsg := string(contentBytes)
		safeSend(cb.channel, ResultCallback{Error: ParseBamlError(errMsg)})
	} else {
		decoded, err := decodeResult(contentBytes)
		if err != nil {
			safeSend(cb.channel, ResultCallback{Error: err})
		} else {
			safeSend(cb.channel, ResultCallback{Data: decoded})
		}
	}

	safeClose(cb.channel)
	deleteCallback(uint32(id))
}
