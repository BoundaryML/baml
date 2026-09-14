package baml_go

import (
	"context"
	"fmt"
	"runtime"
	"strings"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// Image, Audio, Video, and Pdf are owned BAML media values. Copies of one Go
// value share the same underlying handle safely. The handle remains live until
// the last copy becomes unreachable; values may be passed back into BAML any
// number of times because each call serializes the canonical media payload.
type Image struct{ media mediaValue }
type Audio struct{ media mediaValue }
type Video struct{ media mediaValue }
type Pdf struct{ media mediaValue }

type mediaKind uint8

const (
	mediaKindImage mediaKind = iota + 1
	mediaKindAudio
	mediaKindVideo
	mediaKindPDF
)

type mediaHandle struct {
	key        uint64
	handleType cffi.BamlHandleType
}

type mediaValue struct {
	handle *mediaHandle
	kind   mediaKind
}

var (
	cloneInboundHandle          = nativeHandleClone
	accessInboundMedia          = nativeMediaAccess
	releaseInboundHandle        = nativeHandleRelease
	cloneOutboundMediaHandle    = nativeHandleClone
	constructPortableMedia      = constructMedia
	releaseMediaHandle          = nativeHandleRelease
	releaseOutboundHandle       = nativeHandleRelease
	validateOutboundMediaHandle = func(key uint64, handleType cffi.BamlHandleType) error {
		_, err := nativeMediaAccess(mediaMIMEType, key, handleType)
		return err
	}
)

func ownOutboundHandles(value *cffi.BamlOutboundValue) *resultOwner {
	keys := make(map[uint64]struct{})
	collectOutboundHandles(value, keys)
	return ownOutboundHandleKeys(keys)
}

func ownOutboundHandleKeys(keys map[uint64]struct{}) *resultOwner {
	if len(keys) == 0 {
		return nil
	}
	owner := &resultOwner{keys: make([]uint64, 0, len(keys))}
	for key := range keys {
		owner.keys = append(owner.keys, key)
	}
	runtime.SetFinalizer(owner, finalizeResultOwner)
	return owner
}

func releaseOutboundHandles(value *cffi.BamlOutboundValue) {
	owner := ownOutboundHandles(value)
	releaseResultOwner(owner)
}

func releaseResultOwner(owner *resultOwner) {
	if owner == nil {
		return
	}
	runtime.SetFinalizer(owner, nil)
	finalizeResultOwner(owner)
}

func finalizeResultOwner(owner *resultOwner) {
	if owner == nil {
		return
	}
	for _, key := range owner.keys {
		if key != 0 {
			releaseOutboundHandle(key)
		}
	}
	owner.keys = nil
}

func collectOutboundHandles(value *cffi.BamlOutboundValue, keys map[uint64]struct{}) {
	stack := []*cffi.BamlOutboundValue{value}
	for len(stack) > 0 {
		last := len(stack) - 1
		value = stack[last]
		stack = stack[:last]
		if value == nil {
			continue
		}
		switch item := value.Value.(type) {
		case *cffi.BamlOutboundValue_HandleValue:
			if item.HandleValue == nil || item.HandleValue.Key == 0 {
				continue
			}
			handleType := item.HandleValue.HandleType
			if handleType != cffi.BamlHandleType_HOST_VALUE_CALLABLE && handleType != cffi.BamlHandleType_HOST_VALUE_OPAQUE {
				keys[item.HandleValue.Key] = struct{}{}
			}
		case *cffi.BamlOutboundValue_ClassValue:
			if item.ClassValue != nil {
				for _, field := range item.ClassValue.Fields {
					if field != nil {
						stack = append(stack, field.Value)
					}
				}
			}
		case *cffi.BamlOutboundValue_ListValue:
			if item.ListValue != nil {
				stack = append(stack, item.ListValue.Items...)
			}
		case *cffi.BamlOutboundValue_MapValue:
			if item.MapValue != nil {
				for _, entry := range item.MapValue.Entries {
					if entry != nil {
						stack = append(stack, entry.Value)
					}
				}
			}
		case *cffi.BamlOutboundValue_UnionVariantValue:
			if item.UnionVariantValue != nil {
				stack = append(stack, item.UnionVariantValue.Value)
			}
		}
	}
}

// releaseOutboundOpaqueHostValues releases Go-native error identities only
// after an error/panic envelope has been fully formatted. These handles are
// not native handle-table entries and deliberately remain excluded from the
// ordinary result owner; callable handles may have independent live uses and
// are never reclaimed here.
func releaseOutboundOpaqueHostValues(value *cffi.BamlOutboundValue) {
	keys := make(map[uint64]struct{})
	stack := []*cffi.BamlOutboundValue{value}
	for len(stack) > 0 {
		last := len(stack) - 1
		value = stack[last]
		stack = stack[:last]
		if value == nil {
			continue
		}
		switch item := value.Value.(type) {
		case *cffi.BamlOutboundValue_HandleValue:
			if item.HandleValue != nil && item.HandleValue.Key != 0 && item.HandleValue.HandleType == cffi.BamlHandleType_HOST_VALUE_OPAQUE {
				keys[item.HandleValue.Key] = struct{}{}
			}
		case *cffi.BamlOutboundValue_ClassValue:
			if item.ClassValue != nil {
				for _, field := range item.ClassValue.Fields {
					if field != nil {
						stack = append(stack, field.Value)
					}
				}
			}
		case *cffi.BamlOutboundValue_ListValue:
			if item.ListValue != nil {
				stack = append(stack, item.ListValue.Items...)
			}
		case *cffi.BamlOutboundValue_MapValue:
			if item.MapValue != nil {
				for _, entry := range item.MapValue.Entries {
					if entry != nil {
						stack = append(stack, entry.Value)
					}
				}
			}
		case *cffi.BamlOutboundValue_UnionVariantValue:
			if item.UnionVariantValue != nil {
				stack = append(stack, item.UnionVariantValue.Value)
			}
		}
	}
	for key := range keys {
		unregisterHostValue(key)
	}
}

type mediaConstructor uint8

const (
	mediaFromURL mediaConstructor = iota + 1
	mediaFromFile
	mediaFromBase64
)

func (operation mediaConstructor) String() string {
	switch operation {
	case mediaFromURL:
		return "create BAML media from URL"
	case mediaFromFile:
		return "create BAML media from file"
	case mediaFromBase64:
		return "create BAML media from base64"
	default:
		return fmt.Sprintf("create BAML media (operation %d)", operation)
	}
}

type mediaAccessor uint8

const (
	mediaURL mediaAccessor = iota + 1
	mediaFile
	mediaBase64
	mediaMIMEType
)

func (operation mediaAccessor) String() string {
	switch operation {
	case mediaURL:
		return "read BAML media URL"
	case mediaFile:
		return "read BAML media file"
	case mediaBase64:
		return "read BAML media base64"
	case mediaMIMEType:
		return "read BAML media MIME type"
	default:
		return fmt.Sprintf("read BAML media (operation %d)", operation)
	}
}

func nativeHandleStatus(operation string, status uint32) error {
	if status == 0 {
		return nil
	}
	detail := "unknown status"
	switch status {
	case 1:
		detail = "invalid handle"
	case 2:
		detail = "handle type mismatch"
	case 3:
		detail = "unsupported handle type"
	case 4:
		detail = "internal error"
	case 5:
		detail = "unexpected null pointer"
	}
	return fmt.Errorf("%s: %s (%d)", operation, detail, status)
}

func validateNativeMediaBuffer(hasPointer bool, length uint64) error {
	if !hasPointer && length != 0 {
		return fmt.Errorf("BAML media accessor returned a nil buffer with length %d", length)
	}
	if length > uint64(1<<31-1) {
		return fmt.Errorf("BAML media accessor returned an oversized buffer of %d bytes", length)
	}
	return nil
}

func mediaConstructorKind(kind mediaKind) cffi.MediaTypeEnum {
	// The constructor ABI shares MediaTypeEnum's historical order, in which
	// PDF precedes Video. BamlTyMediaKind uses the semantic order below.
	switch kind {
	case mediaKindImage:
		return cffi.MediaTypeEnum_IMAGE
	case mediaKindAudio:
		return cffi.MediaTypeEnum_AUDIO
	case mediaKindPDF:
		return cffi.MediaTypeEnum_PDF
	case mediaKindVideo:
		return cffi.MediaTypeEnum_VIDEO
	default:
		return cffi.MediaTypeEnum_MEDIA_TYPE_UNSPECIFIED
	}
}

func mediaDescriptorKind(kind mediaKind) cffi.BamlTyMediaKind {
	switch kind {
	case mediaKindImage:
		return cffi.BamlTyMediaKind_BAML_TY_MEDIA_KIND_IMAGE
	case mediaKindAudio:
		return cffi.BamlTyMediaKind_BAML_TY_MEDIA_KIND_AUDIO
	case mediaKindVideo:
		return cffi.BamlTyMediaKind_BAML_TY_MEDIA_KIND_VIDEO
	case mediaKindPDF:
		return cffi.BamlTyMediaKind_BAML_TY_MEDIA_KIND_PDF
	default:
		return cffi.BamlTyMediaKind_BAML_TY_MEDIA_KIND_UNSPECIFIED
	}
}

func expectedMediaHandle(kind mediaKind) cffi.BamlHandleType {
	switch kind {
	case mediaKindImage:
		return cffi.BamlHandleType_ADT_MEDIA_IMAGE
	case mediaKindAudio:
		return cffi.BamlHandleType_ADT_MEDIA_AUDIO
	case mediaKindVideo:
		return cffi.BamlHandleType_ADT_MEDIA_VIDEO
	case mediaKindPDF:
		return cffi.BamlHandleType_ADT_MEDIA_PDF
	default:
		return cffi.BamlHandleType_HANDLE_UNSPECIFIED
	}
}

func mediaClassName(kind mediaKind) string {
	switch kind {
	case mediaKindImage:
		return "baml.media.Image"
	case mediaKindAudio:
		return "baml.media.Audio"
	case mediaKindVideo:
		return "baml.media.Video"
	case mediaKindPDF:
		return "baml.media.Pdf"
	default:
		return ""
	}
}

func constructMedia(operation mediaConstructor, kind mediaKind, value string, mimeType *string) (mediaValue, error) {
	if strings.IndexByte(value, 0) >= 0 {
		return mediaValue{}, fmt.Errorf("%s: value contains a NUL byte", operation)
	}
	if mimeType != nil && strings.IndexByte(*mimeType, 0) >= 0 {
		return mediaValue{}, fmt.Errorf("%s: MIME type contains a NUL byte", operation)
	}
	if err := ensureNativeRuntime(context.Background()); err != nil {
		return mediaValue{}, err
	}
	key, handleType, err := nativeMediaConstruct(operation, mediaConstructorKind(kind), value, mimeType)
	if err != nil {
		return mediaValue{}, err
	}
	if key == 0 {
		return mediaValue{}, fmt.Errorf("%s: runtime returned a zero handle", operation)
	}
	if expected := expectedMediaHandle(kind); handleType != expected {
		nativeHandleRelease(key)
		return mediaValue{}, fmt.Errorf("%s: runtime returned handle type %d, expected %d", operation, handleType, expected)
	}
	handle := &mediaHandle{key: key, handleType: handleType}
	runtime.SetFinalizer(handle, finalizeMediaHandle)
	return mediaValue{handle: handle, kind: kind}, nil
}

func mediaFromOwnedHandle(key uint64, handleType cffi.BamlHandleType, kind mediaKind) (mediaValue, error) {
	if key == 0 {
		return mediaValue{}, fmt.Errorf("BAML returned a zero media handle")
	}
	if expected := expectedMediaHandle(kind); handleType != expected {
		return mediaValue{}, fmt.Errorf("BAML returned media handle type %d, expected %d", handleType, expected)
	}
	if err := validateOutboundMediaHandle(key, handleType); err != nil {
		return mediaValue{}, fmt.Errorf("validate BAML media handle: %w", err)
	}
	cloned, err := cloneOutboundMediaHandle(key)
	if err != nil {
		return mediaValue{}, err
	}
	handle := &mediaHandle{key: cloned, handleType: handleType}
	runtime.SetFinalizer(handle, finalizeMediaHandle)
	return mediaValue{handle: handle, kind: kind}, nil
}

func finalizeMediaHandle(handle *mediaHandle) {
	if handle != nil && handle.key != 0 {
		releaseMediaHandle(handle.key)
		handle.key = 0
	}
}

func (media mediaValue) validate(kind mediaKind) error {
	if media.handle == nil || media.handle.key == 0 {
		return fmt.Errorf("uninitialized BAML media value")
	}
	if media.kind != kind || media.handle.handleType != expectedMediaHandle(kind) {
		return fmt.Errorf("BAML media kind does not match its Go type")
	}
	return nil
}

func (media mediaValue) input(kind mediaKind) Input {
	if err := media.validate(kind); err != nil {
		return InvalidInput(err.Error())
	}
	handle := media.handle
	return Input{deferred: &inputEncoder{encode: func(transaction *inputTransaction) (*cffi.InboundValue, error) {
		_ = transaction // media is serialized data; no handle lifetime transfers.
		payload := &cffi.BamlValueMedia{Media: mediaConstructorKind(kind)}
		mimeType, err := accessInboundMedia(mediaMIMEType, handle.key, handle.handleType)
		if err != nil {
			return nil, err
		}
		payload.MimeType = mimeType

		url, err := accessInboundMedia(mediaURL, handle.key, handle.handleType)
		if err != nil {
			return nil, err
		}
		if url != nil {
			payload.Value = &cffi.BamlValueMedia_Url{Url: *url}
		} else {
			file, err := accessInboundMedia(mediaFile, handle.key, handle.handleType)
			if err != nil {
				return nil, err
			}
			if file != nil {
				payload.Value = &cffi.BamlValueMedia_File{File: *file}
			} else {
				base64, err := accessInboundMedia(mediaBase64, handle.key, handle.handleType)
				if err != nil {
					return nil, err
				}
				if base64 == nil {
					return nil, fmt.Errorf("BAML media value has no portable payload")
				}
				payload.Value = &cffi.BamlValueMedia_Base64{Base64: *base64}
			}
		}
		runtime.KeepAlive(handle)
		return &cffi.InboundValue{Value: &cffi.InboundValue_MediaValue{MediaValue: payload}}, nil
	}}}
}

func (media mediaValue) access(kind mediaKind, operation mediaAccessor) (*string, error) {
	if err := media.validate(kind); err != nil {
		return nil, err
	}
	value, err := nativeMediaAccess(operation, media.handle.key, media.handle.handleType)
	runtime.KeepAlive(media.handle)
	return value, err
}

func newImage(operation mediaConstructor, value string, mimeType *string) (Image, error) {
	media, err := constructMedia(operation, mediaKindImage, value, mimeType)
	return Image{media: media}, err
}
func newAudio(operation mediaConstructor, value string, mimeType *string) (Audio, error) {
	media, err := constructMedia(operation, mediaKindAudio, value, mimeType)
	return Audio{media: media}, err
}
func newVideo(operation mediaConstructor, value string, mimeType *string) (Video, error) {
	media, err := constructMedia(operation, mediaKindVideo, value, mimeType)
	return Video{media: media}, err
}
func newPdf(operation mediaConstructor, value string, mimeType *string) (Pdf, error) {
	media, err := constructMedia(operation, mediaKindPDF, value, mimeType)
	return Pdf{media: media}, err
}

func NewImageFromUrl(value string, mimeType *string) (Image, error) {
	return newImage(mediaFromURL, value, mimeType)
}
func NewImageFromFile(value string, mimeType *string) (Image, error) {
	return newImage(mediaFromFile, value, mimeType)
}
func NewImageFromBase64(value string, mimeType *string) (Image, error) {
	return newImage(mediaFromBase64, value, mimeType)
}
func NewAudioFromUrl(value string, mimeType *string) (Audio, error) {
	return newAudio(mediaFromURL, value, mimeType)
}
func NewAudioFromFile(value string, mimeType *string) (Audio, error) {
	return newAudio(mediaFromFile, value, mimeType)
}
func NewAudioFromBase64(value string, mimeType *string) (Audio, error) {
	return newAudio(mediaFromBase64, value, mimeType)
}
func NewVideoFromUrl(value string, mimeType *string) (Video, error) {
	return newVideo(mediaFromURL, value, mimeType)
}
func NewVideoFromFile(value string, mimeType *string) (Video, error) {
	return newVideo(mediaFromFile, value, mimeType)
}
func NewVideoFromBase64(value string, mimeType *string) (Video, error) {
	return newVideo(mediaFromBase64, value, mimeType)
}
func NewPdfFromUrl(value string, mimeType *string) (Pdf, error) {
	return newPdf(mediaFromURL, value, mimeType)
}
func NewPdfFromFile(value string, mimeType *string) (Pdf, error) {
	return newPdf(mediaFromFile, value, mimeType)
}
func NewPdfFromBase64(value string, mimeType *string) (Pdf, error) {
	return newPdf(mediaFromBase64, value, mimeType)
}

func ImageInput(value Image) Input { return value.media.input(mediaKindImage) }
func AudioInput(value Audio) Input { return value.media.input(mediaKindAudio) }
func VideoInput(value Video) Input { return value.media.input(mediaKindVideo) }
func PdfInput(value Pdf) Input     { return value.media.input(mediaKindPDF) }

func (value Image) BAMLInput() Input { return ImageInput(value) }
func (value Audio) BAMLInput() Input { return AudioInput(value) }
func (value Video) BAMLInput() Input { return VideoInput(value) }
func (value Pdf) BAMLInput() Input   { return PdfInput(value) }

func mediaBAMLType(kind mediaKind) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Media{Media: &cffi.BamlTyMedia{Kind: mediaDescriptorKind(kind)}}}}
}
func ImageBAMLType() BAMLType    { return mediaBAMLType(mediaKindImage) }
func AudioBAMLType() BAMLType    { return mediaBAMLType(mediaKindAudio) }
func VideoBAMLType() BAMLType    { return mediaBAMLType(mediaKindVideo) }
func PdfBAMLType() BAMLType      { return mediaBAMLType(mediaKindPDF) }
func (Image) BAMLType() BAMLType { return ImageBAMLType() }
func (Audio) BAMLType() BAMLType { return AudioBAMLType() }
func (Video) BAMLType() BAMLType { return VideoBAMLType() }
func (Pdf) BAMLType() BAMLType   { return PdfBAMLType() }

func (value Image) Url() (*string, error)  { return value.media.access(mediaKindImage, mediaURL) }
func (value Image) File() (*string, error) { return value.media.access(mediaKindImage, mediaFile) }
func (value Image) Base64() (string, error) {
	return requiredMediaString(value.media.access(mediaKindImage, mediaBase64))
}
func (value Image) MimeType() (*string, error) {
	return value.media.access(mediaKindImage, mediaMIMEType)
}
func (value Audio) Url() (*string, error)  { return value.media.access(mediaKindAudio, mediaURL) }
func (value Audio) File() (*string, error) { return value.media.access(mediaKindAudio, mediaFile) }
func (value Audio) Base64() (string, error) {
	return requiredMediaString(value.media.access(mediaKindAudio, mediaBase64))
}
func (value Audio) MimeType() (*string, error) {
	return value.media.access(mediaKindAudio, mediaMIMEType)
}
func (value Video) Url() (*string, error)  { return value.media.access(mediaKindVideo, mediaURL) }
func (value Video) File() (*string, error) { return value.media.access(mediaKindVideo, mediaFile) }
func (value Video) Base64() (string, error) {
	return requiredMediaString(value.media.access(mediaKindVideo, mediaBase64))
}
func (value Video) MimeType() (*string, error) {
	return value.media.access(mediaKindVideo, mediaMIMEType)
}
func (value Pdf) Url() (*string, error)  { return value.media.access(mediaKindPDF, mediaURL) }
func (value Pdf) File() (*string, error) { return value.media.access(mediaKindPDF, mediaFile) }
func (value Pdf) Base64() (string, error) {
	return requiredMediaString(value.media.access(mediaKindPDF, mediaBase64))
}
func (value Pdf) MimeType() (*string, error) { return value.media.access(mediaKindPDF, mediaMIMEType) }

func requiredMediaString(value *string, err error) (string, error) {
	if err != nil {
		return "", err
	}
	if value == nil {
		return "", nil
	}
	return *value, nil
}

func decodeMedia(value Value, kind mediaKind) (mediaValue, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return mediaValue{}, err
	}
	value = unwrapped
	if value.value == nil {
		return mediaValue{}, fmt.Errorf("BAML value is uninitialized")
	}
	var handle *cffi.BamlOutboundHandle
	switch item := value.value.Value.(type) {
	case *cffi.BamlOutboundValue_MediaValue:
		if item.MediaValue == nil {
			return mediaValue{}, fmt.Errorf("BAML media value is empty")
		}
		expected := cffi.MediaTypeEnum(kind)
		if kind == mediaKindPDF {
			expected = cffi.MediaTypeEnum_PDF
		} else if kind == mediaKindVideo {
			expected = cffi.MediaTypeEnum_VIDEO
		}
		if item.MediaValue.Media != expected {
			return mediaValue{}, fmt.Errorf("expected BAML media kind %s, got %s", expected, item.MediaValue.Media)
		}
		var operation mediaConstructor
		var content string
		switch encoded := item.MediaValue.Value.(type) {
		case *cffi.BamlValueMedia_Url:
			operation, content = mediaFromURL, encoded.Url
		case *cffi.BamlValueMedia_File:
			operation, content = mediaFromFile, encoded.File
		case *cffi.BamlValueMedia_Base64:
			operation, content = mediaFromBase64, encoded.Base64
		default:
			return mediaValue{}, fmt.Errorf("BAML media value has no content")
		}
		return constructPortableMedia(operation, kind, content, item.MediaValue.MimeType)
	case *cffi.BamlOutboundValue_HandleValue:
		handle = item.HandleValue
	case *cffi.BamlOutboundValue_ClassValue:
		if item.ClassValue == nil || item.ClassValue.Name != mediaClassName(kind) {
			name := ""
			if item.ClassValue != nil {
				name = item.ClassValue.Name
			}
			return mediaValue{}, fmt.Errorf("expected BAML media class %q, got %q", mediaClassName(kind), name)
		}
		if len(item.ClassValue.TypeArgs) != 0 {
			return mediaValue{}, fmt.Errorf("BAML media class %q unexpectedly has %d type arguments", mediaClassName(kind), len(item.ClassValue.TypeArgs))
		}
		if len(item.ClassValue.Fields) != 1 {
			return mediaValue{}, fmt.Errorf("BAML media class %q must contain exactly one _data field, got %d fields", mediaClassName(kind), len(item.ClassValue.Fields))
		}
		field := item.ClassValue.Fields[0]
		if field == nil {
			return mediaValue{}, fmt.Errorf("BAML media class %q has an empty _data field entry", mediaClassName(kind))
		}
		if field.Key != "_data" {
			return mediaValue{}, fmt.Errorf("BAML media class %q expected field _data, got %q", mediaClassName(kind), field.Key)
		}
		if field.Value == nil {
			return mediaValue{}, fmt.Errorf("BAML media class %q field _data has an empty value", mediaClassName(kind))
		}
		encoded, ok := field.Value.Value.(*cffi.BamlOutboundValue_HandleValue)
		if !ok {
			return mediaValue{}, fmt.Errorf("BAML media class %q field _data is not a handle", mediaClassName(kind))
		}
		handle = encoded.HandleValue
	default:
		return mediaValue{}, fmt.Errorf("expected BAML media value, got %T", value.value.Value)
	}
	if handle == nil {
		return mediaValue{}, fmt.Errorf("BAML media class %q is missing its _data handle", mediaClassName(kind))
	}
	media, err := mediaFromOwnedHandle(handle.Key, handle.HandleType, kind)
	runtime.KeepAlive(value.owner)
	return media, err
}

func (value Value) Image() (Image, error) {
	media, err := decodeMedia(value, mediaKindImage)
	return Image{media: media}, err
}
func (value Value) Audio() (Audio, error) {
	media, err := decodeMedia(value, mediaKindAudio)
	return Audio{media: media}, err
}
func (value Value) Video() (Video, error) {
	media, err := decodeMedia(value, mediaKindVideo)
	return Video{media: media}, err
}
func (value Value) Pdf() (Pdf, error) {
	media, err := decodeMedia(value, mediaKindPDF)
	return Pdf{media: media}, err
}
