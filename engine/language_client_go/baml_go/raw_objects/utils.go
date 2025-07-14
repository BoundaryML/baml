package raw_objects

import (
	"fmt"
	"runtime"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/serde"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
	"google.golang.org/protobuf/proto"
)

/*
#cgo CFLAGS: -I${SRCDIR}/../../include
#cgo CFLAGS: -O3 -g
#include <../baml_cffi_wrapper.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
*/
import "C"

type rawPointer interface {
	objectType() cffi.CFFIObjectType
	pointer() int64
}

type rawObject struct {
	ptr int64     // pointer to the raw object in C
	_   [0]func() // prevents copying
}

func (r *rawObject) pointer() int64 {
	return r.ptr
}

// newRawObject creates a new refcounted rawObject
func newRawObject(objectType cffi.CFFIObjectType, kwargs []*cffi.CFFIMapEntry) (any, error) {
	args := cffi.CFFIObjectConstructorArgs{
		Type:   objectType,
		Kwargs: kwargs,
	}

	encodedArgs, err := proto.Marshal(&args)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal object constructor arguments: %w", err)
	}
	cEncodedArgs := (*C.char)(unsafe.Pointer(&encodedArgs[0]))

	cBuf := C.WrapCallObjectConstructor(cEncodedArgs, C.uintptr_t(len(encodedArgs)))

	content_bytes := C.GoBytes(unsafe.Pointer(cBuf.ptr), C.int32_t(cBuf.len))
	C.WrapFreeBuffer(cBuf) // Free the buffer after use

	if cBuf.len == 0 {
		return nil, fmt.Errorf("failed to call object constructor")
	}
	if cBuf.ptr == nil {
		return nil, fmt.Errorf("object constructor returned nil pointer")
	}

	var content_holder cffi.CFFIObjectResponse
	err = proto.Unmarshal(content_bytes, &content_holder)
	if err != nil {
		return nil, fmt.Errorf("failed to unmarshal content bytes: %w", err)
	}
	parsed, err := decodeObjectResponse(&content_holder)
	if err != nil {
		return nil, fmt.Errorf("failed to decode object response: %w", err)
	}

	return parsed, nil
}

func destructor(object rawPointer) error {
	result, err := callMethod(object, "destructor", nil)

	if err != nil {
		return fmt.Errorf("failed to call destructor: %w", err)
	}

	if result != nil {
		return fmt.Errorf("destructor returned unexpected result: %v", result)
	}

	return nil
}

func callMethod(object rawPointer, method_name string, kwargs map[string]any) (any, error) {
	cffi_kwargs, err := serde.EncodeMapEntries(kwargs, "function arguments")
	if err != nil {
		return nil, fmt.Errorf("encoding method arguments: %w", err)
	}

	args := cffi.CFFIObjectMethodArguments{
		Kwargs:     cffi_kwargs,
		Object:     encodeRawObject(object),
		MethodName: method_name,
	}

	encodedArgs, err := proto.Marshal(&args)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal object method arguments: %w", err)
	}
	cEncodedArgs := (*C.char)(unsafe.Pointer(&encodedArgs[0]))

	cBuf := C.WrapCallObjectMethodFunction(cEncodedArgs, C.uintptr_t(len(encodedArgs)))

	content_bytes := C.GoBytes(unsafe.Pointer(cBuf.ptr), C.int32_t(cBuf.len))
	C.WrapFreeBuffer(cBuf) // Free the buffer after use
	if cBuf.len == 0 {
		return nil, fmt.Errorf("failed to call object method function")
	}
	if cBuf.ptr == nil {
		return nil, fmt.Errorf("object method function returned nil pointer")
	}

	var content_holder cffi.CFFIObjectResponse
	err = proto.Unmarshal(content_bytes, &content_holder)
	if err != nil {
		return nil, fmt.Errorf("failed to unmarshal content bytes: %w", err)
	}

	parsed, err := decodeObjectResponse(&content_holder)
	if err != nil {
		return nil, fmt.Errorf("failed to decode object response: %w", err)
	}

	return parsed, nil
}

func decodeObjectResponse(response *cffi.CFFIObjectResponse) (any, error) {
	if response == nil {
		return nil, fmt.Errorf("nil response")
	}

	switch response.GetResponse().(type) {
	case *cffi.CFFIObjectResponse_Error:
		return nil, fmt.Errorf("%s", response.GetError().Error)
	case *cffi.CFFIObjectResponse_Success:
		success := response.GetSuccess()
		switch success.Result.(type) {
		case *cffi.CFFIObjectResponseSuccess_Object:
			object := success.GetObject()
			return decodeRawObject(object)
		case *cffi.CFFIObjectResponseSuccess_Objects:
			objects := success.GetObjects()
			parsed := make([]rawPointer, len(objects.Objects))
			for i, obj := range objects.Objects {
				decoded, err := decodeRawObject(obj)
				if err != nil {
					return nil, fmt.Errorf("failed to decode object at index %d: %w", i, err)
				}
				parsed[i] = decoded
			}
			return parsed, nil
		case *cffi.CFFIObjectResponseSuccess_Value:
			value := success.GetValue()
			return serde.Decode(value, nil).Interface(), nil
		default:
			panic("unexpected cffi.isCFFIObjectResponseSuccess_Result")
		}
	default:
		panic("unexpected cffi.isCFFIObjectResponse_Response")
	}
}

func decodeRawObject(cRaw *cffi.CFFIRawObject) (rawPointer, error) {
	raw, err := decodeRawObjectImpl(cRaw)
	if err != nil {
		return nil, err
	}
	// on finalization, we need to call the destructor
	runtime.SetFinalizer(raw, func(r rawPointer) {
		if err := destructor(r); err != nil {
			fmt.Printf("Error during finalization of raw object: %v\n", err)
		}
	})

	return raw, nil
}

func decodeRawObjectImpl(cRaw *cffi.CFFIRawObject) (rawPointer, error) {
	if cRaw == nil {
		return nil, fmt.Errorf("nil raw object")
	}

	switch obj := cRaw.Object.(type) {
	case *cffi.CFFIRawObject_Collector:
		return newCollector(obj.Collector.Pointer), nil
	case *cffi.CFFIRawObject_FunctionLog:
		return newFunctionLog(obj.FunctionLog.Pointer), nil
	case *cffi.CFFIRawObject_HttpBody:
		return newHTTPBody(obj.HttpBody.Pointer), nil
	case *cffi.CFFIRawObject_HttpRequest:
		return newHttpRequest(obj.HttpRequest.Pointer), nil
	case *cffi.CFFIRawObject_HttpResponse:
		return newHttpResponse(obj.HttpResponse.Pointer), nil
	case *cffi.CFFIRawObject_LlmCall:
		return newLLMCall(obj.LlmCall.Pointer), nil
	case *cffi.CFFIRawObject_LlmStreamCall:
		return newLLMStreamCall(obj.LlmStreamCall.Pointer), nil
	case *cffi.CFFIRawObject_SseResponse:
		return newSSEResponse(obj.SseResponse.Pointer), nil
	case *cffi.CFFIRawObject_StreamTiming:
		return newStreamTiming(obj.StreamTiming.Pointer), nil
	case *cffi.CFFIRawObject_Timing:
		return newTiming(obj.Timing.Pointer), nil
	case *cffi.CFFIRawObject_Usage:
		return newUsage(obj.Usage.Pointer), nil
	default:
		return nil, fmt.Errorf("unexpected raw object type")
	}
}

func encodeRawObject(object rawPointer) *cffi.CFFIRawObject {
	pointer := &cffi.CFFIPointerType{
		Pointer: object.pointer(),
	}

	switch object.objectType() {
	case cffi.CFFIObjectType_OBJECT_COLLECTOR:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_Collector{
				Collector: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_FUNCTION_LOG:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_FunctionLog{
				FunctionLog: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_HTTP_BODY:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_HttpBody{
				HttpBody: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_HTTP_REQUEST:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_HttpRequest{
				HttpRequest: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_HTTP_RESPONSE:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_HttpResponse{
				HttpResponse: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_LLM_CALL:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_LlmCall{
				LlmCall: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_LLM_STREAM_CALL:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_LlmStreamCall{
				LlmStreamCall: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_SSE_RESPONSE:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_SseResponse{
				SseResponse: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_STREAM_TIMING:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_StreamTiming{
				StreamTiming: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_TIMING:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_Timing{
				Timing: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_USAGE:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_Usage{
				Usage: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_UNSPECIFIED:
		panic("unexpected cffi.CFFIObjectType_OBJECT_UNSPECIFIED")
	default:
		panic("unexpected cffi.CFFIObjectType")
	}
}
