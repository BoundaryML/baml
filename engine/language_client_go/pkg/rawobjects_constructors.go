package baml

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/baml_go/serde"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

/// Construct Collector
func (r *BamlRuntime) NewCollector(name string) (Collector, error) {
	kwargs, err := serde.EncodeMapEntries(map[string]any{
		"name": name,
	}, "collector constructor args")
	if err != nil {
		return nil, fmt.Errorf("failed to encode kwargs: %w", err)
	}

	ptr, err := raw_objects.NewRawObject(r.runtime, cffi.CFFIObjectType_OBJECT_COLLECTOR, kwargs)
	if err != nil {
		return nil, fmt.Errorf("failed to create collector: %w", err)
	}

	as_collector, ok := ptr.(*collector)
	if !ok {
		return nil, fmt.Errorf("unexpected type for collector creation: %T", ptr)
	}

	return as_collector, nil
}


func (r *BamlRuntime) newMediaFromUrl(mediaType MediaType, url string, mimeType *string) (media, error) {
	kwargs, err := serde.EncodeMapEntries(map[string]any{
		"mime_type": mimeType,
		"url":       url,
	}, "media constructor args")
	if err != nil {
		return nil, fmt.Errorf("failed to encode kwargs: %w", err)
	}

	ptr, err := raw_objects.NewRawObject(r.runtime, mediaType.objectType(), kwargs)
	if err != nil {
		return nil, fmt.Errorf("failed to create media: %w", err)
	}

	as_media, ok := ptr.(media)
	if !ok {
		return nil, fmt.Errorf("unexpected type for media creation: %T", ptr)
	}

	return as_media, nil
}

func (r *BamlRuntime) NewImageFromUrl(url string, mimeType *string) (Image, error) {
	return r.newMediaFromUrl(MediaType_Image, url, mimeType)
}

func (r *BamlRuntime) NewAudioFromUrl(url string, mimeType *string) (Audio, error) {
	return r.newMediaFromUrl(MediaType_Audio, url, mimeType)
}

func (r *BamlRuntime) NewPDFFromUrl(url string, mimeType *string) (PDF, error) {
	return r.newMediaFromUrl(MediaType_PDF, url, mimeType)
}

func (r *BamlRuntime) NewVideoFromUrl(url string, mimeType *string) (Video, error) {
	return r.newMediaFromUrl(MediaType_Video, url, mimeType)
}

func (r *BamlRuntime) newMediaFromBase64(mediaType MediaType, base64 string, mimeType *string) (media, error) {
	kwargs, err := serde.EncodeMapEntries(map[string]any{
		"mime_type": mimeType,
		"base64":    base64,
	}, "media constructor args")
	if err != nil {
		return nil, fmt.Errorf("failed to encode kwargs: %w", err)
	}

	ptr, err := raw_objects.NewRawObject(r.runtime, mediaType.objectType(), kwargs)
	if err != nil {
		return nil, fmt.Errorf("failed to create media: %w", err)
	}

	as_media, ok := ptr.(media)
	if !ok {
		return nil, fmt.Errorf("unexpected type for media creation: %T", ptr)
	}

	return as_media, nil
}

func (r *BamlRuntime) NewImageFromBase64(base64 string, mimeType *string) (Image, error) {
	return r.newMediaFromBase64(MediaType_Image, base64, mimeType)
}

func (r *BamlRuntime) NewAudioFromBase64(base64 string, mimeType *string) (Audio, error) {
	return r.newMediaFromBase64(MediaType_Audio, base64, mimeType)
}

func (r *BamlRuntime) NewPDFFromBase64(base64 string, mimeType *string) (PDF, error) {
	return r.newMediaFromBase64(MediaType_PDF, base64, mimeType)
}

func (r *BamlRuntime) NewVideoFromBase64(base64 string, mimeType *string) (Video, error) {
	return r.newMediaFromBase64(MediaType_Video, base64, mimeType)
}