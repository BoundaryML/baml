package baml

import (
	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type streamTiming struct {
	*timing
}

func newStreamTiming(ptr int64) StreamTiming {
	return &streamTiming{&timing{raw_objects.FromPointer(ptr)}}
}

func (s *streamTiming) objectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_STREAM_TIMING
}

func (s *streamTiming) pointer() int64 {
	return s.timing.RawObject.Pointer()
}
