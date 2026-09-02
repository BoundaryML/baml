package baml_go

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"runtime"
	"strings"
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
)

var (
	_ func(string, *string) (Image, error) = NewImageFromUrl
	_ func(string, *string) (Image, error) = NewImageFromFile
	_ func(string, *string) (Image, error) = NewImageFromBase64
	_ func(string, *string) (Audio, error) = NewAudioFromUrl
	_ func(string, *string) (Audio, error) = NewAudioFromFile
	_ func(string, *string) (Audio, error) = NewAudioFromBase64
	_ func(string, *string) (Video, error) = NewVideoFromUrl
	_ func(string, *string) (Video, error) = NewVideoFromFile
	_ func(string, *string) (Video, error) = NewVideoFromBase64
	_ func(string, *string) (Pdf, error)   = NewPdfFromUrl
	_ func(string, *string) (Pdf, error)   = NewPdfFromFile
	_ func(string, *string) (Pdf, error)   = NewPdfFromBase64
)

func TestMediaDescriptorsAreExactAndDistinct(t *testing.T) {
	types := []struct {
		name string
		got  BAMLType
		kind cffi.BamlTyMediaKind
	}{
		{"image", ImageBAMLType(), cffi.BamlTyMediaKind_BAML_TY_MEDIA_KIND_IMAGE},
		{"audio", AudioBAMLType(), cffi.BamlTyMediaKind_BAML_TY_MEDIA_KIND_AUDIO},
		{"video", VideoBAMLType(), cffi.BamlTyMediaKind_BAML_TY_MEDIA_KIND_VIDEO},
		{"pdf", PdfBAMLType(), cffi.BamlTyMediaKind_BAML_TY_MEDIA_KIND_PDF},
	}
	for _, test := range types {
		media := test.got.value.GetMedia()
		if media == nil || media.Kind != test.kind {
			t.Fatalf("%s descriptor = %#v, want kind %s", test.name, media, test.kind)
		}
	}
	for left := range types {
		for right := left + 1; right < len(types); right++ {
			if types[left].got.Equal(types[right].got) {
				t.Fatalf("%s and %s descriptors compare equal", types[left].name, types[right].name)
			}
		}
	}
}

func TestMediaPublicAPIHasNoInitialismSpecialCases(t *testing.T) {
	file, err := parser.ParseFile(token.NewFileSet(), "media.go", nil, 0)
	if err != nil {
		t.Fatal(err)
	}
	check := func(name string) {
		t.Helper()
		for _, forbidden := range []string{"URL", "MIME", "PDF"} {
			if strings.Contains(name, forbidden) {
				t.Fatalf("exported media API %q special-cases initialism %q", name, forbidden)
			}
		}
	}
	for _, declaration := range file.Decls {
		switch declaration := declaration.(type) {
		case *ast.FuncDecl:
			if ast.IsExported(declaration.Name.Name) {
				check(declaration.Name.Name)
			}
		case *ast.GenDecl:
			for _, spec := range declaration.Specs {
				if typeSpec, ok := spec.(*ast.TypeSpec); ok && ast.IsExported(typeSpec.Name.Name) {
					check(typeSpec.Name.Name)
				}
			}
		}
	}
}

func TestZeroMediaValuesFailEncodingWithoutPanic(t *testing.T) {
	inputs := []Input{ImageInput(Image{}), AudioInput(Audio{}), VideoInput(Video{}), PdfInput(Pdf{})}
	for index, input := range inputs {
		if input.err == nil || input.value != nil {
			t.Fatalf("zero media input %d = %#v, want an error", index, input)
		}
	}
}

func TestCollectOutboundHandlesFindsNestedOwnedHandlesOnlyOnce(t *testing.T) {
	owned := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_HandleValue{HandleValue: &cffi.BamlOutboundHandle{
		Key: 41, HandleType: cffi.BamlHandleType_ADT_MEDIA_IMAGE,
	}}}
	hostCallable := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_HandleValue{HandleValue: &cffi.BamlOutboundHandle{
		Key: 99, HandleType: cffi.BamlHandleType_HOST_VALUE_CALLABLE,
	}}}
	hostOpaque := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_HandleValue{HandleValue: &cffi.BamlOutboundHandle{
		Key: 100, HandleType: cffi.BamlHandleType_HOST_VALUE_OPAQUE,
	}}}
	union := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_UnionVariantValue{UnionVariantValue: &cffi.BamlValueUnionVariant{
		Value: owned,
	}}}
	mapValue := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_MapValue{MapValue: &cffi.BamlValueMap{
		Entries: []*cffi.BamlOutboundMapEntry{{Key: "union", Value: union}},
	}}}
	root := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{
		Fields: []*cffi.BamlOutboundMapEntry{
			{Key: "first", Value: owned},
			{Key: "list", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{ListValue: &cffi.BamlValueList{Items: []*cffi.BamlOutboundValue{owned, hostCallable, hostOpaque}}}}},
			{Key: "map", Value: mapValue},
		},
	}}}
	keys := make(map[uint64]struct{})
	collectOutboundHandles(root, keys)
	if len(keys) != 1 {
		t.Fatalf("owned keys = %#v", keys)
	}
	if _, ok := keys[41]; !ok {
		t.Fatalf("owned keys = %#v, missing media handle", keys)
	}
}

func TestCollectOutboundHandlesDoesNotDropDeeplyNestedHandles(t *testing.T) {
	value := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_HandleValue{HandleValue: &cffi.BamlOutboundHandle{
		Key: 42, HandleType: cffi.BamlHandleType_ADT_MEDIA_IMAGE,
	}}}
	for range 1024 {
		value = &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{ListValue: &cffi.BamlValueList{
			Items: []*cffi.BamlOutboundValue{value},
		}}}
	}
	keys := make(map[uint64]struct{})
	collectOutboundHandles(value, keys)
	if _, ok := keys[42]; !ok || len(keys) != 1 {
		t.Fatalf("owned keys = %#v, want only deeply nested handle 42", keys)
	}
}

func TestDecodeMediaRejectsWrongEnvelopeBeforeNativeClone(t *testing.T) {
	handle := func(key uint64, kind cffi.BamlHandleType) *cffi.BamlOutboundValue {
		return &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_HandleValue{HandleValue: &cffi.BamlOutboundHandle{Key: key, HandleType: kind}}}
	}
	imageHandle := handle(1, cffi.BamlHandleType_ADT_MEDIA_IMAGE)
	tests := []Value{
		{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{Name: "baml.media.Audio"}}}},
		{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{Name: "baml.media.Image"}}}},
		{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{Name: "baml.media.Image", TypeArgs: []*cffi.BamlTy{PrimitiveBAMLType(StringType).value}, Fields: []*cffi.BamlOutboundMapEntry{{Key: "_data", Value: imageHandle}}}}}},
		{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{Name: "baml.media.Image", Fields: []*cffi.BamlOutboundMapEntry{nil}}}}},
		{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{Name: "baml.media.Image", Fields: []*cffi.BamlOutboundMapEntry{{Key: "not_data", Value: imageHandle}}}}}},
		{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{Name: "baml.media.Image", Fields: []*cffi.BamlOutboundMapEntry{{Key: "_data"}}}}}},
		{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{Name: "baml.media.Image", Fields: []*cffi.BamlOutboundMapEntry{{Key: "_data", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "not handle"}}}}}}}},
		{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{Name: "baml.media.Image", Fields: []*cffi.BamlOutboundMapEntry{{Key: "_data", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_HandleValue{}}}}}}}},
		{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{Name: "baml.media.Image", Fields: []*cffi.BamlOutboundMapEntry{{Key: "_data", Value: imageHandle}, {Key: "extra", Value: imageHandle}}}}}},
		{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{Name: "baml.media.Image", Fields: []*cffi.BamlOutboundMapEntry{{Key: "_data", Value: imageHandle}, {Key: "_data", Value: imageHandle}}}}}},
		{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "not media"}}},
	}
	for index, value := range tests {
		if _, err := value.Image(); err == nil {
			t.Fatalf("invalid media envelope %d decoded", index)
		}
	}
}

func TestDecodeMediaValidatesActualNativeKeyKindBeforeClone(t *testing.T) {
	previousValidate := validateOutboundMediaHandle
	previousClone := cloneOutboundMediaHandle
	cloneCalls := 0
	actualKinds := map[uint64]cffi.BamlHandleType{
		1: cffi.BamlHandleType_ADT_MEDIA_AUDIO,
		2: cffi.BamlHandleType_FUNCTION_REF,
		3: cffi.BamlHandleType_ADT_MEDIA_IMAGE,
	}
	validateOutboundMediaHandle = func(key uint64, expected cffi.BamlHandleType) error {
		if actualKinds[key] != expected {
			return fmt.Errorf("native key kind %d does not match %d", actualKinds[key], expected)
		}
		return nil
	}
	cloneOutboundMediaHandle = func(key uint64) (uint64, error) {
		cloneCalls++
		return key + 100, nil
	}
	t.Cleanup(func() {
		validateOutboundMediaHandle = previousValidate
		cloneOutboundMediaHandle = previousClone
	})

	encoded := func(key uint64) Value {
		return Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_HandleValue{HandleValue: &cffi.BamlOutboundHandle{
			Key: key, HandleType: cffi.BamlHandleType_ADT_MEDIA_IMAGE,
		}}}}
	}
	for _, key := range []uint64{1, 2} {
		if _, err := encoded(key).Image(); err == nil {
			t.Fatalf("mismatched native key %d decoded", key)
		}
	}
	if cloneCalls != 0 {
		t.Fatalf("mismatched keys were cloned %d times", cloneCalls)
	}
	image, err := encoded(3).Image()
	if err != nil {
		t.Fatal(err)
	}
	if cloneCalls != 1 || image.media.handle.key != 103 {
		t.Fatalf("valid key clone = %d / %#v", cloneCalls, image)
	}
	// The injected clone key is not registered in the real native table.
	// Disable its production finalizer before the test value becomes unreachable.
	runtime.SetFinalizer(image.media.handle, nil)
}

func TestDecodePortableMediaReconstructsTheTypedWrapper(t *testing.T) {
	previous := constructPortableMedia
	defer func() { constructPortableMedia = previous }()
	constructPortableMedia = func(operation mediaConstructor, kind mediaKind, value string, mimeType *string) (mediaValue, error) {
		if operation != mediaFromURL || kind != mediaKindImage || value != "https://example.test/cat.png" {
			t.Fatalf("portable media constructor = %v / %v / %q", operation, kind, value)
		}
		if mimeType == nil || *mimeType != "image/png" {
			t.Fatalf("portable media MIME type = %#v", mimeType)
		}
		return mediaValue{
			handle: &mediaHandle{key: 77, handleType: cffi.BamlHandleType_ADT_MEDIA_IMAGE},
			kind:   mediaKindImage,
		}, nil
	}

	value := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_MediaValue{
		MediaValue: &cffi.BamlValueMedia{
			Media:    cffi.MediaTypeEnum_IMAGE,
			MimeType: stringPointer("image/png"),
			Value:    &cffi.BamlValueMedia_Url{Url: "https://example.test/cat.png"},
		},
	}}}
	image, err := value.Image()
	if err != nil {
		t.Fatal(err)
	}
	if image.media.handle == nil || image.media.handle.key != 77 {
		t.Fatalf("decoded portable media = %#v", image)
	}
}

func stringPointer(value string) *string { return &value }

func TestNativeMediaBufferValidationRejectsImpossibleShapes(t *testing.T) {
	if err := validateNativeMediaBuffer(false, 1); err == nil {
		t.Fatal("nil pointer with nonzero length was accepted")
	}
	if err := validateNativeMediaBuffer(true, uint64(1<<31)); err == nil {
		t.Fatal("buffer larger than C.int was accepted")
	}
	for _, test := range []struct {
		hasPointer bool
		length     uint64
	}{{false, 0}, {true, 0}, {true, 1<<31 - 1}} {
		if err := validateNativeMediaBuffer(test.hasPointer, test.length); err != nil {
			t.Fatalf("valid buffer shape %#v rejected: %v", test, err)
		}
	}
}
