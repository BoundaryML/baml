package main

import (
	"context"
	"media_types/baml_client"
	"testing"

	"media_types/baml_client/types"

	baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
)

func TestImageInput(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	imageMedia, err := baml.NewImageFromUrl(
		"https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png",
		nil,
	)
	if err != nil {
		t.Fatalf("Error creating image: %v", err)
	}

	union := types.Union4AudioOrImageOrPDFOrVideo__NewImage(imageMedia)

	result, err := baml_client.TestMediaInput(ctx, union, "Analyze this image")
	if err != nil {
		t.Fatalf("Error testing image input: %v", err)
	}

	if result.AnalysisText == "" {
		t.Errorf("Expected analysis text to be non-empty")
	}
}

func TestAudioInput(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	cr := baml.ClientRegistry{}
	cr.SetPrimaryClient("openai/gpt-4o-audio-preview")

	audioMedia, err := baml.NewAudioFromUrl(
		"https://download.samplelib.com/mp3/sample-3s.mp3",
		nil,
	)
	if err != nil {
		t.Fatalf("Error creating audio: %v", err)
	}

	union := types.Union4AudioOrImageOrPDFOrVideo__NewAudio(audioMedia)

	result, err := baml_client.TestMediaInput(ctx, union, "This is music used for an intro", baml_client.WithClientRegistry(&cr))
	if err != nil {
		t.Fatalf("Error testing audio input: %v", err)
	}

	if result.AnalysisText == "" {
		t.Errorf("Expected analysis text to be non-empty")
	}
}

func TestPdfInput(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	pdfMedia, err := baml.NewPDFFromUrl(
		"https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf",
		nil,
	)
	if err != nil {
		t.Fatalf("Error creating PDF: %v", err)
	}

	union := types.Union4AudioOrImageOrPDFOrVideo__NewPDF(pdfMedia)

	result, err := baml_client.TestMediaInput(ctx, union, "Analyze this PDF")
	if err != nil {
		t.Fatalf("Error testing PDF input: %v", err)
	}

	if result.AnalysisText == "" {
		t.Errorf("Expected analysis text to be non-empty")
	}
}

func TestVideoInput(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	videoMedia, err := baml.NewVideoFromUrl(
		"https://www.youtube.com/watch?v=1O0yazhqaxs",
		nil,
	)
	if err != nil {
		t.Fatalf("Error creating video: %v", err)
	}

	union := types.Union4AudioOrImageOrPDFOrVideo__NewVideo(videoMedia)

	cr := baml.ClientRegistry{}
	cr.SetPrimaryClient("google-ai/gemini-2.5-flash")

	result, err := baml_client.TestMediaInput(ctx, union, "Analyze this video", baml_client.WithClientRegistry(&cr))
	if err != nil {
		t.Fatalf("Error testing video input: %v", err)
	}

	if result.AnalysisText == "" {
		t.Errorf("Expected analysis text to be non-empty")
	}
}

func TestImageArrayInput(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	image1, err := baml.NewImageFromUrl(
		"https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png",
		nil,
	)
	if err != nil {
		t.Fatalf("Error creating first image: %v", err)
	}

	image2, err := baml.NewImageFromUrl(
		"https://upload.wikimedia.org/wikipedia/commons/thumb/a/a7/React-icon.svg/1200px-React-icon.svg.png",
		nil,
	)
	if err != nil {
		t.Fatalf("Error creating second image: %v", err)
	}

	imageArray := []types.Image{image1, image2}

	result, err := baml_client.TestMediaArrayInputs(ctx, imageArray, "Analyze these images")
	if err != nil {
		t.Fatalf("Error testing image array inputs: %v", err)
	}

	if result.MediaCount <= 0 {
		t.Errorf("Expected media count to be positive, got %d", result.MediaCount)
	}
	if result.AnalysisText == "" {
		t.Errorf("Expected analysis text to be non-empty")
	}
}
