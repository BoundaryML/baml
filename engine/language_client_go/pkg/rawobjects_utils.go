package baml

import (
	"fmt"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

func decodeRawObjectImpl(rt unsafe.Pointer, cRaw *cffi.BamlObjectHandle) (raw_objects.RawPointer, error) {
	if cRaw == nil {
		return nil, fmt.Errorf("nil raw object")
	}

	switch obj := cRaw.Object.(type) {
	case *cffi.BamlObjectHandle_Collector:
		return newCollector(obj.Collector.Pointer, rt), nil
	case *cffi.BamlObjectHandle_FunctionLog:
		return newFunctionLog(obj.FunctionLog.Pointer, rt), nil
	case *cffi.BamlObjectHandle_HttpBody:
		return newHTTPBody(obj.HttpBody.Pointer, rt), nil
	case *cffi.BamlObjectHandle_HttpRequest:
		return newHttpRequest(obj.HttpRequest.Pointer, rt), nil
	case *cffi.BamlObjectHandle_HttpResponse:
		return newHttpResponse(obj.HttpResponse.Pointer, rt), nil
	case *cffi.BamlObjectHandle_LlmCall:
		return newLLMCall(obj.LlmCall.Pointer, rt), nil
	case *cffi.BamlObjectHandle_LlmStreamCall:
		return newLLMStreamCall(obj.LlmStreamCall.Pointer, rt), nil
	case *cffi.BamlObjectHandle_SseResponse:
		return newSSEResponse(obj.SseResponse.Pointer, rt), nil
	case *cffi.BamlObjectHandle_StreamTiming:
		return newStreamTiming(obj.StreamTiming.Pointer, rt), nil
	case *cffi.BamlObjectHandle_Timing:
		return newTiming(obj.Timing.Pointer, rt), nil
	case *cffi.BamlObjectHandle_Usage:
		return newUsage(obj.Usage.Pointer, rt), nil
	case *cffi.BamlObjectHandle_MediaImage:
		return newMedia(obj.MediaImage.Pointer, rt, MediaType_Image), nil
	case *cffi.BamlObjectHandle_MediaAudio:
		return newMedia(obj.MediaAudio.Pointer, rt, MediaType_Audio), nil
	case *cffi.BamlObjectHandle_MediaPdf:
		return newMedia(obj.MediaPdf.Pointer, rt, MediaType_PDF), nil
	case *cffi.BamlObjectHandle_MediaVideo:
		return newMedia(obj.MediaVideo.Pointer, rt, MediaType_Video), nil
	case *cffi.BamlObjectHandle_Type:
		return newType(obj.Type.Pointer, rt), nil
	case *cffi.BamlObjectHandle_TypeBuilder:
		return newTypeBuilder(obj.TypeBuilder.Pointer, rt), nil
	case *cffi.BamlObjectHandle_EnumBuilder:
		return newEnumBuilder(obj.EnumBuilder.Pointer, rt), nil
	case *cffi.BamlObjectHandle_ClassBuilder:
		return newClassBuilder(obj.ClassBuilder.Pointer, rt), nil
	case *cffi.BamlObjectHandle_EnumValueBuilder:
		return newEnumValueBuilder(obj.EnumValueBuilder.Pointer, rt), nil
	case *cffi.BamlObjectHandle_ClassPropertyBuilder:
		return newClassPropertyBuilder(obj.ClassPropertyBuilder.Pointer, rt), nil
	default:
		return nil, fmt.Errorf("unexpected raw object type %T", cRaw.Object)
	}
}

func init() {
	raw_objects.SetDecodeRawObjectImpl(decodeRawObjectImpl)
}
