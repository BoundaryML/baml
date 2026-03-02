package baml

/*
#include <stdlib.h>
#include <stdint.h>
*/
import "C"

import (
	"context"
	"fmt"
	"os"
	"reflect"
	"sync"
	"sync/atomic"
	"time"
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

// responseType constants for callback dispatch
const (
	responseTypeValue        = "value"         // Default: decode as CFFIValueHolder
	responseTypeObjectHandle = "object_handle" // Decode as InvocationResponse with BamlObjectHandle
)

type CallbackData struct {
	channel      chan ResultCallback
	ctx          context.Context
	onTick       OnTickCallbackData
	responseType string         // "value" (default) or "object_handle"
	runtime      unsafe.Pointer // runtime pointer, needed for object handle decoding
}

type OnTickCallbackData interface {
	Collector() Collector
	OnTick() TickCallback
}

// Map to store callbacks by ID
var (
	dynamicCallbacks  = make(map[uint32]CallbackData)
	callbackMutex     sync.RWMutex
	nextCallbackID    atomic.Uint32
	typeMap           serde.TypeMap
	callbackLogFile   *os.File
	callbackLogFileMu sync.Mutex
	callbackLogOnce   sync.Once
)

// getCallbackLogFile returns the log file for client callback events, or nil if logging is disabled
func getCallbackLogFile() *os.File {
	callbackLogOnce.Do(func() {
		if path := os.Getenv("BAML_FFI_CLIENT_LOG"); path != "" {
			f, err := os.OpenFile(path, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0644)
			if err == nil {
				callbackLogFile = f
			}
		}
	})
	return callbackLogFile
}

// callbackLog writes a log message to the Go FFI log file with timestamp
func callbackLog(format string, args ...any) {
	if f := getCallbackLogFile(); f != nil {
		callbackLogFileMu.Lock()
		defer callbackLogFileMu.Unlock()
		ts := time.Now().UnixMicro()
		msg := fmt.Sprintf(format, args...)
		// Insert timestamp after the opening bracket
		if len(msg) > 0 && msg[0] == '[' {
			bracketEnd := 0
			for i, c := range msg {
				if c == ']' {
					bracketEnd = i
					break
				}
			}
			fmt.Fprintf(f, "%s ts=%d%s\n", msg[:bracketEnd], ts, msg[bracketEnd:])
		} else {
			fmt.Fprintf(f, "ts=%d %s\n", ts, msg)
		}
	}
}

func SetTypeMap(t map[string]reflect.Type) {
	typeMap = serde.NewExternalTypeMap(t)
}

//export on_tick_callback
func on_tick_callback(id C.uint32_t) {
	defer func() {
		if r := recover(); r != nil {
			callbackLog("[CLIENT_GO_CALLBACK_PANIC] on_tick id=%d panic=%v", uint32(id), r)
		}
	}()

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

		// Send the error to the callback
		if content_str == "AbortError" {
			// Special handling for AbortError
			safeSend(callback.channel, ResultCallback{Error: callback.ctx.Err()})
		} else {
			// TODO: cast to the right error type
			err := BamlError{Message: content_str}
			safeSend(callback.channel, ResultCallback{Error: err})
		}

		safeClose(callback.channel)
		callbackMutex.Lock()
		defer callbackMutex.Unlock()
		deleteCallback(id_uint)
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

		if callback.responseType == responseTypeObjectHandle {
			trigger_callback_object_handle(id_uint, callback, content_bytes)
			return
		}

		var content_holder cffi.CFFIValueHolder
		err := proto.Unmarshal(content_bytes, &content_holder)
		if err != nil {
			safeSend(callback.channel, ResultCallback{Error: err})
			safeClose(callback.channel)
			callbackMutex.Lock()
			defer callbackMutex.Unlock()
			deleteCallback(id_uint)
			return
		}

		raw_decoded_data, _ := serde.Decode(&content_holder, typeMap)
		var decoded_data interface{}
		if raw_decoded_data.IsValid() {
			decoded_data = raw_decoded_data.Interface()
		}

		var res ResultCallback
		if isDone == 1 {
			res = ResultCallback{HasData: true, Data: decoded_data}
		} else {
			res = ResultCallback{HasStreamData: true, StreamData: decoded_data}
		}

		safeSend(callback.channel, res)
		if isDone == 1 {
			safeClose(callback.channel)
			callbackMutex.Lock()
			defer callbackMutex.Unlock()
			deleteCallback(id_uint)
		}
	}
}

// trigger_callback_object_handle handles callbacks that return an InvocationResponse
// containing a BamlObjectHandle (e.g. from build_request_from_c).
func trigger_callback_object_handle(id uint32, callback CallbackData, content_bytes []byte) {
	var response cffi.InvocationResponse
	err := proto.Unmarshal(content_bytes, &response)
	if err != nil {
		safeSend(callback.channel, ResultCallback{Error: fmt.Errorf("failed to unmarshal InvocationResponse: %w", err)})
		safeClose(callback.channel)
		callbackMutex.Lock()
		defer callbackMutex.Unlock()
		deleteCallback(id)
		return
	}

	switch resp := response.GetResponse().(type) {
	case *cffi.InvocationResponse_Error:
		safeSend(callback.channel, ResultCallback{Error: BamlError{Message: resp.Error}})
	case *cffi.InvocationResponse_Success:
		success := resp.Success
		if success == nil {
			safeSend(callback.channel, ResultCallback{Error: fmt.Errorf("nil success in InvocationResponse")})
		} else {
			switch result := success.GetResult().(type) {
			case *cffi.InvocationResponseSuccess_Object:
				decoded, decodeErr := decodeRawObjectImpl(callback.runtime, result.Object)
				if decodeErr != nil {
					safeSend(callback.channel, ResultCallback{Error: fmt.Errorf("failed to decode object handle: %w", decodeErr)})
				} else {
					safeSend(callback.channel, ResultCallback{HasData: true, Data: decoded})
				}
			default:
				safeSend(callback.channel, ResultCallback{Error: fmt.Errorf("unexpected result type in InvocationResponse: %T", success.GetResult())})
			}
		}
	default:
		safeSend(callback.channel, ResultCallback{Error: fmt.Errorf("unexpected response type in InvocationResponse")})
	}

	safeClose(callback.channel)
	callbackMutex.Lock()
	defer callbackMutex.Unlock()
	deleteCallback(id)
}

func create_unique_id(ctx context.Context, onTick OnTickCallbackData) (uint32, chan ResultCallback) {
	id := nextCallbackID.Add(1)
	callbackMutex.Lock()
	defer callbackMutex.Unlock()
	dynamicCallbacks[id] = CallbackData{channel: make(chan ResultCallback, 64), ctx: ctx, onTick: onTick, responseType: responseTypeValue}
	callbackLog("[CLIENT_GO_CALLBACK_ADD] id=%d map_size=%d", id, len(dynamicCallbacks))
	return id, dynamicCallbacks[id].channel
}

// create_unique_id_for_object creates a callback ID for object-handle responses (e.g. build_request).
// The runtime pointer is needed to decode the object handle on the Go side.
func create_unique_id_for_object(ctx context.Context, runtime unsafe.Pointer) (uint32, chan ResultCallback) {
	id := nextCallbackID.Add(1)
	callbackMutex.Lock()
	defer callbackMutex.Unlock()
	dynamicCallbacks[id] = CallbackData{
		channel:      make(chan ResultCallback, 1),
		ctx:          ctx,
		responseType: responseTypeObjectHandle,
		runtime:      runtime,
	}
	callbackLog("[CLIENT_GO_CALLBACK_ADD] id=%d type=object_handle map_size=%d", id, len(dynamicCallbacks))
	return id, dynamicCallbacks[id].channel
}

// safeSend sends a result on a callback channel, recovering from panic if
// the channel has already been closed by a concurrent callback (e.g.
// error_callback closing the channel while trigger_callback is sending).
func safeSend(ch chan ResultCallback, res ResultCallback) (sent bool) {
	defer func() {
		if r := recover(); r != nil {
			callbackLog("[CLIENT_GO_CALLBACK_PANIC] safeSend panic=%v", r)
			sent = false
		}
	}()
	ch <- res
	return true
}

// safeClose closes a callback channel, recovering from panic if it has
// already been closed by a concurrent callback.
func safeClose(ch chan ResultCallback) {
	defer func() {
		if r := recover(); r != nil {
			callbackLog("[CLIENT_GO_CALLBACK_PANIC] safeClose panic=%v", r)
		}
	}()
	close(ch)
}

// cleanupCallback safely closes the channel and removes the callback entry.
// Used by runtime.go when a C FFI call fails synchronously — the Rust side
// won't invoke error_callback/trigger_callback, so we must clean up ourselves.
func cleanupCallback(id uint32, ch chan ResultCallback) {
	safeClose(ch)
	callbackMutex.Lock()
	defer callbackMutex.Unlock()
	deleteCallback(id)
}

// Helper to log callback removal
func deleteCallback(id uint32) {
	delete(dynamicCallbacks, id)
	callbackLog("[CLIENT_GO_CALLBACK_DEL] id=%d map_size=%d", id, len(dynamicCallbacks))
}
