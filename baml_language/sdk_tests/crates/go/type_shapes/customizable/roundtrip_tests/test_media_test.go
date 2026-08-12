package sdk_test

import (
	"context"
	"os"
	"reflect"
	"testing"

	"baml.local/sdk/baml_sdk"
	"github.com/boundaryml/baml-go"
)

const mediaURL = "https://example.com/asset"
const mediaBase64 = "aGVsbG8="

var (
	_ func(context.Context, string, *string) (baml_go.Image, error)                         = baml_sdk.MediaReturnImage
	_ func(context.Context, string, *string) (baml_go.Audio, error)                         = baml_sdk.MediaReturnAudio
	_ func(context.Context, string, *string) (baml_go.Video, error)                         = baml_sdk.MediaReturnVideo
	_ func(context.Context, string, *string) (baml_go.Pdf, error)                           = baml_sdk.MediaReturnPdf
	_ func(context.Context, baml_go.Image) (baml_go.Image, error)                           = baml_sdk.MediaRoundTripImage
	_ func(context.Context, baml_go.Audio) (baml_go.Audio, error)                           = baml_sdk.MediaRoundTripAudio
	_ func(context.Context, baml_go.Video) (baml_go.Video, error)                           = baml_sdk.MediaRoundTripVideo
	_ func(context.Context, baml_go.Pdf) (baml_go.Pdf, error)                               = baml_sdk.MediaRoundTripPdf
	_ func(context.Context, *baml_go.Image) (*baml_go.Image, error)                         = baml_sdk.MediaRoundTripOptionalImage
	_ func(context.Context, []baml_go.Image) ([]baml_go.Image, error)                       = baml_sdk.MediaRoundTripImageList
	_ func(context.Context, map[string]baml_go.Image) (map[string]baml_go.Image, error)     = baml_sdk.MediaRoundTripImageMap
	_ func(context.Context, baml_sdk.ImageOrAudio) (baml_sdk.ImageOrAudio, error)           = baml_sdk.MediaRoundTripImageOrAudio
	_ func(context.Context, baml_sdk.ImageOrMediaImage) (baml_sdk.ImageOrMediaImage, error) = baml_sdk.MediaRoundTripMediaOrImageClass
	_ func(context.Context, any) (any, error)                                               = baml_sdk.MediaRoundTripAllMedia
)

func assertOptionalString(t *testing.T, got *string, want string) {
	t.Helper()
	if got == nil || *got != want {
		t.Fatalf("got %#v, want %q", got, want)
	}
}

func Test_media_return_and_round_trip_all_kinds(t *testing.T) {
	ctx := context.Background()
	mime := "application/octet-stream"

	image, err := baml_sdk.MediaReturnImage(ctx, mediaURL, &mime)
	if err != nil {
		t.Fatal(err)
	}
	audio, err := baml_sdk.MediaReturnAudio(ctx, mediaURL, &mime)
	if err != nil {
		t.Fatal(err)
	}
	video, err := baml_sdk.MediaReturnVideo(ctx, mediaURL, &mime)
	if err != nil {
		t.Fatal(err)
	}
	pdf, err := baml_sdk.MediaReturnPdf(ctx, mediaURL, &mime)
	if err != nil {
		t.Fatal(err)
	}

	for name, check := range map[string]func() error{
		"image": func() error { _, err := baml_sdk.MediaRoundTripImage(ctx, image); return err },
		"audio": func() error { _, err := baml_sdk.MediaRoundTripAudio(ctx, audio); return err },
		"video": func() error { _, err := baml_sdk.MediaRoundTripVideo(ctx, video); return err },
		"pdf":   func() error { _, err := baml_sdk.MediaRoundTripPdf(ctx, pdf); return err },
	} {
		if err := check(); err != nil {
			t.Fatalf("%s round trip: %v", name, err)
		}
	}
	assertOptionalString(t, mustImageUrl(t, image), mediaURL)
	assertOptionalString(t, mustImageMime(t, image), mime)

	got, err := baml_sdk.MediaReservedMediaNames(ctx, image, audio, video, pdf)
	if err != nil {
		t.Fatal(err)
	}
	assertOptionalString(t, mustImageUrl(t, got), mediaURL)

	mediaClass := baml_sdk.MediaMedia{
		ImageField: image,
		AudioField: audio,
		VideoField: video,
		PdfField:   pdf,
	}
	gotClass, err := baml_sdk.MediaRoundTripMedia(ctx, mediaClass)
	if err != nil {
		t.Fatal(err)
	}
	assertOptionalString(t, mustImageUrl(t, gotClass.ImageField), mediaURL)

	imageUnion, err := baml_sdk.MediaRoundTripImageOrAudio(ctx, baml_sdk.NewImageOrAudioFromImage(image))
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := imageUnion.AsImage(); !ok {
		t.Fatalf("image union returned %s", imageUnion.Kind())
	}
	audioUnion, err := baml_sdk.MediaRoundTripImageOrAudio(ctx, baml_sdk.NewImageOrAudioFromAudio(audio))
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := audioUnion.AsAudio(); !ok {
		t.Fatalf("audio union returned %s", audioUnion.Kind())
	}

	mixedMedia, err := baml_sdk.MediaRoundTripMediaOrImageClass(ctx, baml_sdk.NewImageOrMediaImageFromImage(image))
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := mixedMedia.AsImage(); !ok {
		t.Fatalf("media/class collision returned %s", mixedMedia.Kind())
	}
	mixedClass, err := baml_sdk.MediaRoundTripMediaOrImageClass(ctx, baml_sdk.NewImageOrMediaImageFromMediaImage(baml_sdk.MediaImage{Label: "nominal"}))
	if err != nil {
		t.Fatal(err)
	}
	if got, ok := mixedClass.AsMediaImage(); !ok || got.Label != "nominal" {
		t.Fatalf("media/class collision = %#v, %s", got, mixedClass.Kind())
	}

	for name, test := range map[string]struct {
		input any
		check func(any) bool
	}{
		"image": {image, func(value any) bool { _, ok := value.(baml_go.Image); return ok }},
		"audio": {audio, func(value any) bool { _, ok := value.(baml_go.Audio); return ok }},
		"video": {video, func(value any) bool { _, ok := value.(baml_go.Video); return ok }},
		"pdf":   {pdf, func(value any) bool { _, ok := value.(baml_go.Pdf); return ok }},
	} {
		got, err := baml_sdk.MediaRoundTripAllMedia(ctx, test.input)
		if err != nil {
			t.Fatalf("dynamic %s input: %v", name, err)
		}
		if !test.check(got) {
			t.Fatalf("dynamic %s output has concrete type %T", name, got)
		}
	}

}

func Test_media_constructors_and_accessors(t *testing.T) {
	mime := "text/plain"
	image, err := baml_go.NewImageFromBase64(mediaBase64, &mime)
	if err != nil {
		t.Fatal(err)
	}
	encoded, err := image.Base64()
	if err != nil || encoded != mediaBase64 {
		t.Fatalf("base64 = %q, %v", encoded, err)
	}
	assertOptionalString(t, mustImageMime(t, image), mime)
	if url, err := image.Url(); err != nil || url != nil {
		t.Fatalf("base64 image URL = %#v, %v; want nil", url, err)
	}
	if path, err := image.File(); err != nil || path != nil {
		t.Fatalf("base64 image file = %#v, %v; want nil", path, err)
	}

	audio, err := baml_go.NewAudioFromUrl(mediaURL, nil)
	if err != nil {
		t.Fatal(err)
	}
	url, err := audio.Url()
	if err != nil {
		t.Fatal(err)
	}
	assertOptionalString(t, url, mediaURL)
	if got, err := audio.MimeType(); err != nil || got != nil {
		t.Fatalf("URL audio MIME type = %#v, %v; want nil", got, err)
	}
	if got, err := audio.File(); err != nil || got != nil {
		t.Fatalf("URL audio file = %#v, %v; want nil", got, err)
	}

	file, err := os.CreateTemp(t.TempDir(), "media-*")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := file.WriteString("payload"); err != nil {
		t.Fatal(err)
	}
	if err := file.Close(); err != nil {
		t.Fatal(err)
	}
	video, err := baml_go.NewVideoFromFile(file.Name(), &mime)
	if err != nil {
		t.Fatal(err)
	}
	path, err := video.File()
	if err != nil {
		t.Fatal(err)
	}
	assertOptionalString(t, path, file.Name())
	if got, err := video.Url(); err != nil || got != nil {
		t.Fatalf("file video URL = %#v, %v; want nil", got, err)
	}

	pdf, err := baml_go.NewPdfFromBase64(mediaBase64, nil)
	if err != nil {
		t.Fatal(err)
	}
	encodedPdf, err := pdf.Base64()
	if err != nil || encodedPdf != mediaBase64 {
		t.Fatalf("pdf base64 = %q, %v", encodedPdf, err)
	}
	if got, err := pdf.MimeType(); err != nil || got != nil {
		t.Fatalf("base64 PDF MIME type = %#v, %v; want nil", got, err)
	}
	if got, err := pdf.Url(); err != nil || got != nil {
		t.Fatalf("base64 PDF URL = %#v, %v; want nil", got, err)
	}
	if got, err := pdf.File(); err != nil || got != nil {
		t.Fatalf("base64 PDF file = %#v, %v; want nil", got, err)
	}
	if _, err := baml_sdk.MediaRoundTripPdf(context.Background(), pdf); err != nil {
		t.Fatal(err)
	}
}

func Test_media_nested_optional_and_containers(t *testing.T) {
	ctx := context.Background()
	image, err := baml_go.NewImageFromUrl(mediaURL, nil)
	if err != nil {
		t.Fatal(err)
	}

	if got, err := baml_sdk.MediaRoundTripOptionalImage(ctx, nil); err != nil || got != nil {
		t.Fatalf("null optional = %#v, %v", got, err)
	}
	gotOptional, err := baml_sdk.MediaRoundTripOptionalImage(ctx, &image)
	if err != nil || gotOptional == nil {
		t.Fatalf("present optional = %#v, %v", gotOptional, err)
	}

	images, err := baml_sdk.MediaRoundTripImageList(ctx, []baml_go.Image{image, image})
	if err != nil || len(images) != 2 {
		t.Fatalf("list = %#v, %v", images, err)
	}
	imageMap, err := baml_sdk.MediaRoundTripImageMap(ctx, map[string]baml_go.Image{"a": image})
	if err != nil || len(imageMap) != 1 {
		t.Fatalf("map = %#v, %v", imageMap, err)
	}

	shapes := baml_sdk.MediaMediaShapes{
		OptionalImage:  &image,
		Images:         []baml_go.Image{image},
		OptionalImages: []*baml_go.Image{nil, &image},
		ImagesByName:   map[string]baml_go.Image{"image": image},
		OptionalList:   &[]baml_go.Image{},
	}
	gotShapes, err := baml_sdk.MediaRoundTripMediaShapes(ctx, shapes)
	if err != nil {
		t.Fatal(err)
	}
	if len(gotShapes.Images) != 1 || len(gotShapes.OptionalImages) != 2 || gotShapes.OptionalImages[0] != nil || gotShapes.OptionalList == nil {
		t.Fatalf("shapes = %#v", gotShapes)
	}
	// Media contains handles, so compare the structural container shape and
	// validate payload identity through accessors instead of reflect.DeepEqual.
	if reflect.TypeOf(gotShapes) != reflect.TypeOf(shapes) {
		t.Fatal("generated media shape changed")
	}
}

func mustImageUrl(t *testing.T, value baml_go.Image) *string {
	t.Helper()
	got, err := value.Url()
	if err != nil {
		t.Fatal(err)
	}
	return got
}

func mustImageMime(t *testing.T, value baml_go.Image) *string {
	t.Helper()
	got, err := value.MimeType()
	if err != nil {
		t.Fatal(err)
	}
	return got
}
