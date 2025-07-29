package baml

import (
	"fmt"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type mediaHolder struct {
	*raw_objects.RawObject
	mediaType MediaType
}

type imageHolder struct {
	mediaHolder
}

type audioHolder struct {
	mediaHolder
}

type pdfHolder struct {
	mediaHolder
}

type videoHolder struct {
	mediaHolder
}

// mediaHolder implements InternalBamlSerializer
func (m *mediaHolder) InternalBamlSerializer() {}

func (m *mediaHolder) Encode() (*cffi.CFFIValueHolder, error) {
	mediaType, err := m.mediaType.cffiType()
	if err != nil {
		return nil, err
	}
	return &cffi.CFFIValueHolder{
		Value: &cffi.CFFIValueHolder_MediaValue{
			MediaValue: &cffi.CFFIValueMedia{
				MediaObject: raw_objects.EncodeRawObject(m),
			},
		},
		Type: &cffi.CFFIFieldTypeHolder{
			Type: &cffi.CFFIFieldTypeHolder_MediaType{
				MediaType: &cffi.CFFIFieldTypeMedia{
					Media: mediaType,
				},
			},
		},
	}, nil
}

func (m *imageHolder) Type() (*cffi.CFFIFieldTypeHolder, error) {
	return &cffi.CFFIFieldTypeHolder{
		Type: &cffi.CFFIFieldTypeHolder_MediaType{
			MediaType: &cffi.CFFIFieldTypeMedia{
				Media: cffi.MediaTypeEnum_IMAGE,
			},
		},
	}, nil
}

func (m *audioHolder) Type() (*cffi.CFFIFieldTypeHolder, error) {
	return &cffi.CFFIFieldTypeHolder{
		Type: &cffi.CFFIFieldTypeHolder_MediaType{
			MediaType: &cffi.CFFIFieldTypeMedia{
				Media: cffi.MediaTypeEnum_AUDIO,
			},
		},
	}, nil
}

func (m *pdfHolder) Type() (*cffi.CFFIFieldTypeHolder, error) {
	return &cffi.CFFIFieldTypeHolder{
		Type: &cffi.CFFIFieldTypeHolder_MediaType{
			MediaType: &cffi.CFFIFieldTypeMedia{
				Media: cffi.MediaTypeEnum_PDF,
			},
		},
	}, nil
}

func (m *videoHolder) Type() (*cffi.CFFIFieldTypeHolder, error) {
	return &cffi.CFFIFieldTypeHolder{
		Type: &cffi.CFFIFieldTypeHolder_MediaType{
			MediaType: &cffi.CFFIFieldTypeMedia{
				Media: cffi.MediaTypeEnum_VIDEO,
			},
		},
	}, nil
}

func (mediaType MediaType) objectType() cffi.CFFIObjectType {
	switch mediaType {
	case MediaType_Image:
		return cffi.CFFIObjectType_OBJECT_MEDIA_IMAGE
	case MediaType_Audio:
		return cffi.CFFIObjectType_OBJECT_MEDIA_AUDIO
	case MediaType_PDF:
		return cffi.CFFIObjectType_OBJECT_MEDIA_PDF
	case MediaType_Video:
		return cffi.CFFIObjectType_OBJECT_MEDIA_VIDEO
	default:
		panic(fmt.Sprintf("invalid media type: '%s'", mediaType))
	}
}

func (mediaType MediaType) cffiType() (cffi.MediaTypeEnum, error) {
	switch mediaType {
	case MediaType_Image:
		return cffi.MediaTypeEnum_IMAGE, nil
	case MediaType_Audio:
		return cffi.MediaTypeEnum_AUDIO, nil
	case MediaType_PDF:
		return cffi.MediaTypeEnum_PDF, nil
	case MediaType_Video:
		return cffi.MediaTypeEnum_VIDEO, nil
	default:
		return 0, fmt.Errorf("invalid media type: '%s'", mediaType)
	}
}

func (m *mediaHolder) ObjectType() cffi.CFFIObjectType {
	return m.mediaType.objectType()
}

func (m *mediaHolder) MediaType() (MediaType, error) {
	return m.mediaType, nil
}

func (m *mediaHolder) pointer() int64 {
	return m.RawObject.Pointer()
}

func (m *mediaHolder) MimeType() (*string, error) {
	result, err := raw_objects.CallMethod(m, "mime_type", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get mime type: %w", err)
	}

	if result == nil {
		return nil, nil
	}

	as_mime_type, ok := result.(string)
	if !ok {
		return nil, fmt.Errorf("unexpected type for mime type: %T", result)
	}

	return &as_mime_type, nil
}

func (m *mediaHolder) AsUrl() (*string, error) {
	result, err := raw_objects.CallMethod(m, "as_url", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get as url: %w", err)
	}

	if result == nil {
		return nil, nil
	}

	as_url, ok := result.(string)
	if !ok {
		return nil, fmt.Errorf("unexpected type for as_url: %T", result)
	}

	return &as_url, nil
}
func (m *mediaHolder) AsBase64() (*string, error) {
	result, err := raw_objects.CallMethod(m, "as_base64", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get as base64: %w", err)
	}

	if result == nil {
		return nil, nil
	}

	as_base64, ok := result.(string)
	if !ok {
		return nil, fmt.Errorf("unexpected type for as_base64: %T", result)
	}

	return &as_base64, nil
}

func newMedia(ptr int64, rt unsafe.Pointer, mediaType MediaType) media {
	media := mediaHolder{raw_objects.FromPointer(ptr, rt), mediaType}
	switch mediaType {
	case MediaType_Image:
		return &imageHolder{media}
	case MediaType_Audio:
		return &audioHolder{media}
	case MediaType_PDF:
		return &pdfHolder{media}
	case MediaType_Video:
		return &videoHolder{media}
	default:
		panic(fmt.Sprintf("invalid media type: '%s'", mediaType))
	}
}
