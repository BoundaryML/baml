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
	"os"
	"reflect"
	"sync"
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
	dynamicCallbacks   = make(map[uint32]CallbackData)
	callbackMutex      sync.RWMutex
	typeMap            serde.TypeMap
	callbackLogFile    *os.File
	callbackLogFileMu  sync.Mutex
	callbackLogOnce    sync.Once
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
			callback.channel <- ResultCallback{Error: callback.ctx.Err()}
		} else {
			// TODO: cast to the right error type
			err := BamlError{Message: content_str}
			callback.channel <- ResultCallback{Error: err}
		}

		close(callback.channel)
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

		var content_holder cffi.CFFIValueHolder
		err := proto.Unmarshal(content_bytes, &content_holder)
		if err != nil {
			callback.channel <- ResultCallback{Error: err}
			close(callback.channel)
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

		callback.channel <- res
		if isDone == 1 {
			close(callback.channel)
			callbackMutex.Lock()
			defer callbackMutex.Unlock()
			deleteCallback(id_uint)
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
	callbackLog("[CLIENT_GO_CALLBACK_ADD] id=%d map_size=%d", id, len(dynamicCallbacks))
	return id, dynamicCallbacks[id].channel
}

// Helper to log callback removal
func deleteCallback(id uint32) {
	delete(dynamicCallbacks, id)
	callbackLog("[CLIENT_GO_CALLBACK_DEL] id=%d map_size=%d", id, len(dynamicCallbacks))
}
