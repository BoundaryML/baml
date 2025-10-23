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

var _decodeRawObjectImpl func(rt unsafe.Pointer, cRaw *cffi.CFFIRawObject) (RawPointer, error)

func SetDecodeRawObjectImpl(impl func(rt unsafe.Pointer, cRaw *cffi.CFFIRawObject) (RawPointer, error)) {
	_decodeRawObjectImpl = impl
}

type RawPointer interface {
	ObjectType() cffi.CFFIObjectType
	pointer() int64
	Runtime() unsafe.Pointer
}

type RawObject struct {
	ptr int64     // pointer to the raw object in C
	baml_runtime unsafe.Pointer
	_   [0]func() // prevents copying
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
func NewRawObject(rt unsafe.Pointer, objectType cffi.CFFIObjectType, kwargs []*cffi.CFFIMapEntry) (any, error) {
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

	args := cffi.CFFIObjectMethodArguments{
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

	var content_holder cffi.CFFIObjectResponse
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

func decodeObjectResponse(rt unsafe.Pointer, response *cffi.CFFIObjectResponse) (any, error) {
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
			return decodeRawObject(rt, object)
		case *cffi.CFFIObjectResponseSuccess_Objects:
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
		case *cffi.CFFIObjectResponseSuccess_Value:
			value := success.GetValue()
			return serde.Decode(value, serde.TypeMap{
				"INTERNAL.nil": reflect.TypeOf((*interface{})(nil)).Elem(),
			}).Interface(), nil
		default:
			panic("unexpected cffi.isCFFIObjectResponseSuccess_Result")
		}
	default:
		panic("unexpected cffi.isCFFIObjectResponse_Response")
	}
}

func decodeRawObject(rt unsafe.Pointer, cRaw *cffi.CFFIRawObject) (RawPointer, error) {
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

func EncodeRawObject(object RawPointer) *cffi.CFFIRawObject {
	pointer := &cffi.CFFIPointerType{
		Pointer: object.pointer(),
	}

	switch object.ObjectType() {
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
	case cffi.CFFIObjectType_OBJECT_MEDIA_IMAGE:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_MediaImage{
				MediaImage: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_MEDIA_AUDIO:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_MediaAudio{
				MediaAudio: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_MEDIA_PDF:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_MediaPdf{
				MediaPdf: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_MEDIA_VIDEO:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_MediaVideo{
				MediaVideo: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_TYPE:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_Type{
				Type: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_TYPE_BUILDER:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_TypeBuilder{
				TypeBuilder: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_ENUM_BUILDER:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_EnumBuilder{
				EnumBuilder: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_ENUM_VALUE_BUILDER:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_EnumValueBuilder{
				EnumValueBuilder: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_CLASS_BUILDER:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_ClassBuilder{
				ClassBuilder: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_CLASS_PROPERTY_BUILDER:
		return &cffi.CFFIRawObject{
			Object: &cffi.CFFIRawObject_ClassPropertyBuilder{
				ClassPropertyBuilder: pointer,
			},
		}
	case cffi.CFFIObjectType_OBJECT_UNSPECIFIED:
		panic("unexpected cffi.CFFIObjectType_OBJECT_UNSPECIFIED")
	default:
		panic(fmt.Sprintf("unexpected cffi.CFFIObjectType: %v", object.ObjectType()))
	}
}
