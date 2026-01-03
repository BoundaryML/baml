//go:build cgo

package raw_objects

import (
	"fmt"
	"reflect"
	"runtime"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/serde"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
	"google.golang.org/protobuf/proto"
)

/*
#cgo CFLAGS: -I${SRCDIR}/..
#cgo CFLAGS: -O3 -g
#include <../baml_cffi_wrapper.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
*/
import "C"

var _decodeRawObjectImpl func(rt unsafe.Pointer, cRaw *cffi.BamlObjectHandle) (RawPointer, error)

func SetDecodeRawObjectImpl(impl func(rt unsafe.Pointer, cRaw *cffi.BamlObjectHandle) (RawPointer, error)) {
	_decodeRawObjectImpl = impl
}

type RawPointer interface {
	ObjectType() cffi.BamlObjectType
	pointer() int64
	Runtime() unsafe.Pointer
}

type RawObject struct {
	ptr          int64 // pointer to the raw object in C
	baml_runtime unsafe.Pointer
	_            [0]func() // prevents copying
}

func (r *RawObject) Pointer() int64 {
	return r.pointer()
}

func (r *RawObject) pointer() int64 {
	return r.ptr
}

func (r *RawObject) Runtime() unsafe.Pointer {
	return r.baml_runtime
}

func FromPointer(ptr int64, rt unsafe.Pointer) *RawObject {
	return &RawObject{ptr: ptr, baml_runtime: rt}
}

// newRawObject creates a new refcounted rawObject
func NewRawObject(rt unsafe.Pointer, objectType cffi.BamlObjectType, kwargs []*cffi.HostMapEntry) (any, error) {
	args := cffi.BamlObjectConstructorInvocation{
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

	var content_holder cffi.InvocationResponse
	err = proto.Unmarshal(content_bytes, &content_holder)
	if err != nil {
		return nil, fmt.Errorf("failed to unmarshal content bytes: %w", err)
	}
	parsed, err := decodeObjectResponse(rt, &content_holder)
	if err != nil {
		return nil, fmt.Errorf("failed to decode object response: %w", err)
	}

	return parsed, nil
}

func destructor(object RawPointer) error {
	result, err := CallMethod(object, "~destructor", nil)

	if err != nil {
		return fmt.Errorf("failed to call destructor: %w", err)
	}

	if result != nil {
		return fmt.Errorf("destructor returned unexpected result: %v", result)
	}
	return nil
}

func CallMethod(object RawPointer, method_name string, kwargs map[string]any) (any, error) {
	cffi_kwargs, err := serde.EncodeMapEntries(kwargs, "function arguments")
	if err != nil {
		return nil, fmt.Errorf("encoding method arguments: %w", err)
	}

	args := cffi.BamlObjectMethodInvocation{
		Kwargs:     cffi_kwargs,
		Object:     EncodeRawObject(object),
		MethodName: method_name,
	}

	encodedArgs, err := proto.Marshal(&args)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal object method arguments: %w", err)
	}
	cEncodedArgs := (*C.char)(unsafe.Pointer(&encodedArgs[0]))

	cBuf := C.WrapCallObjectMethodFunction(object.Runtime(), cEncodedArgs, C.uintptr_t(len(encodedArgs)))

	content_bytes := C.GoBytes(unsafe.Pointer(cBuf.ptr), C.int32_t(cBuf.len))
	C.WrapFreeBuffer(cBuf) // Free the buffer after use
	if cBuf.len == 0 {
		return nil, fmt.Errorf("failed to call object method function")
	}
	if cBuf.ptr == nil {
		return nil, fmt.Errorf("object method function returned nil pointer")
	}

	var content_holder cffi.InvocationResponse
	err = proto.Unmarshal(content_bytes, &content_holder)
	if err != nil {
		return nil, fmt.Errorf("failed to unmarshal content bytes: %w", err)
	}

	parsed, err := decodeObjectResponse(object.Runtime(), &content_holder)
	if err != nil {
		return nil, fmt.Errorf("failed to decode object response: %w", err)
	}

	return parsed, nil
}

func decodeObjectResponse(rt unsafe.Pointer, response *cffi.InvocationResponse) (any, error) {
	if response == nil {
		return nil, fmt.Errorf("nil response")
	}

	switch response.GetResponse().(type) {
	case *cffi.InvocationResponse_Error:
		return nil, fmt.Errorf("%s", response.GetError())
	case *cffi.InvocationResponse_Success:
		success := response.GetSuccess()
		switch success.Result.(type) {
		case *cffi.InvocationResponseSuccess_Object:
			object := success.GetObject()
			return decodeRawObject(rt, object)
		case *cffi.InvocationResponseSuccess_Objects:
			objects := success.GetObjects()
			parsed := make([]RawPointer, len(objects.Objects))
			for i, obj := range objects.Objects {
				decoded, err := decodeRawObject(rt, obj)
				if err != nil {
					return nil, fmt.Errorf("failed to decode object at index %d: %w", i, err)
				}
				parsed[i] = decoded
			}
			return parsed, nil
		case *cffi.InvocationResponseSuccess_Value:
			value := success.GetValue()
			decodedValue, _ := serde.Decode(value, serde.TypeMap{
				"INTERNAL.nil": reflect.TypeOf((*interface{})(nil)).Elem(),
			})
			if !decodedValue.IsValid() {
				return nil, nil
			}
			return decodedValue.Interface(), nil
		default:
			panic("unexpected cffi.isCFFIObjectResponseSuccess_Result")
		}
	default:
		panic("unexpected cffi.isCFFIObjectResponse_Response")
	}
}

func decodeRawObject(rt unsafe.Pointer, cRaw *cffi.BamlObjectHandle) (RawPointer, error) {
	if _decodeRawObjectImpl == nil {
		return nil, fmt.Errorf("decodeRawObjectImpl is not set. Please call SetDecodeRawObjectImpl() before using this function")
	}

	raw, err := _decodeRawObjectImpl(rt, cRaw)
	if err != nil {
		return nil, err
	}
	// on finalization, we need to call the destructor
	runtime.SetFinalizer(raw, func(r RawPointer) {
		if err := destructor(r); err != nil {
			fmt.Printf("Error during finalization of raw object (%s): %v\n", r.ObjectType(), err)
		}
	})

	return raw, nil
}

func EncodeRawObject(object RawPointer) *cffi.BamlObjectHandle {
	pointer := &cffi.BamlPointerType{
		Pointer: object.pointer(),
	}

	switch object.ObjectType() {
	case cffi.BamlObjectType_OBJECT_COLLECTOR:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_Collector{
				Collector: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_FUNCTION_LOG:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_FunctionLog{
				FunctionLog: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_HTTP_BODY:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_HttpBody{
				HttpBody: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_HTTP_REQUEST:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_HttpRequest{
				HttpRequest: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_HTTP_RESPONSE:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_HttpResponse{
				HttpResponse: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_LLM_CALL:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_LlmCall{
				LlmCall: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_LLM_STREAM_CALL:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_LlmStreamCall{
				LlmStreamCall: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_SSE_RESPONSE:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_SseResponse{
				SseResponse: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_STREAM_TIMING:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_StreamTiming{
				StreamTiming: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_TIMING:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_Timing{
				Timing: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_USAGE:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_Usage{
				Usage: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_MEDIA_IMAGE:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_MediaImage{
				MediaImage: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_MEDIA_AUDIO:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_MediaAudio{
				MediaAudio: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_MEDIA_PDF:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_MediaPdf{
				MediaPdf: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_MEDIA_VIDEO:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_MediaVideo{
				MediaVideo: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_TYPE:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_Type{
				Type: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_TYPE_BUILDER:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_TypeBuilder{
				TypeBuilder: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_ENUM_BUILDER:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_EnumBuilder{
				EnumBuilder: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_ENUM_VALUE_BUILDER:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_EnumValueBuilder{
				EnumValueBuilder: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_CLASS_BUILDER:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_ClassBuilder{
				ClassBuilder: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_CLASS_PROPERTY_BUILDER:
		return &cffi.BamlObjectHandle{
			Object: &cffi.BamlObjectHandle_ClassPropertyBuilder{
				ClassPropertyBuilder: pointer,
			},
		}
	case cffi.BamlObjectType_OBJECT_UNSPECIFIED:
		panic("unexpected cffi.BamlObjectType_OBJECT_UNSPECIFIED")
	default:
		panic(fmt.Sprintf("unexpected cffi.BamlObjectType: %v", object.ObjectType()))
	}
}
