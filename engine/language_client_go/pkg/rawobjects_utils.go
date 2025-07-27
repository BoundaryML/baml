package baml

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

func decodeRawObjectImpl(cRaw *cffi.CFFIRawObject) (raw_objects.RawPointer, error) {
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
	case *cffi.CFFIRawObject_MediaImage:
		return newMedia(obj.MediaImage.Pointer, MediaType_Image), nil
	case *cffi.CFFIRawObject_MediaAudio:
		return newMedia(obj.MediaAudio.Pointer, MediaType_Audio), nil
	case *cffi.CFFIRawObject_MediaPdf:
		return newMedia(obj.MediaPdf.Pointer, MediaType_PDF), nil
	case *cffi.CFFIRawObject_MediaVideo:
		return newMedia(obj.MediaVideo.Pointer, MediaType_Video), nil
	default:
		return nil, fmt.Errorf("unexpected raw object type")
	}
}

func init() {
	raw_objects.SetDecodeRawObjectImpl(decodeRawObjectImpl)
}
