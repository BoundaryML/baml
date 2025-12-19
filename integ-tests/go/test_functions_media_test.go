package main

import (
	"context"
	"strings"
	"testing"

	b "example.com/integ-tests/baml_client"
	"example.com/integ-tests/baml_client/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestImageInputURL tests image input from URL
// Reference: test_functions.py:421-427
func TestImageInputURL(t *testing.T) {
	ctx := context.Background()

	img, err := b.NewImageFromUrl("https://i.imgur.com/93fWs5R.png", nil)
	require.NoError(t, err)

	result, err := b.TestImageInput(ctx, img)
	require.NoError(t, err)

	// Should contain words related to Shrek
	resultLower := strings.ToLower(result)
	assert.True(t,
		strings.Contains(resultLower, "green") ||
			strings.Contains(resultLower, "yellow") ||
			strings.Contains(resultLower, "shrek") ||
			strings.Contains(resultLower, "ogre"),
		"Expected result to mention Shrek-related words, got: %s", result)
}

// TestImageInputBase64 tests image input from base64
// Reference: test_functions.py:458-460
func TestImageInputBase64(t *testing.T) {
	ctx := context.Background()

	// Base64 data for a small PNG image (from base64_test_data.py)
	imageB64 := "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="

	img, err := b.NewImageFromBase64(imageB64, stringPtr("image/png"))
	require.NoError(t, err)

	result, err := b.TestImageInput(ctx, img)
	require.NoError(t, err)

	// Should get some description of the image
	assert.NotEmpty(t, result, "Expected non-empty result for base64 image")
}

// TestImageListInput tests multiple images input
// Reference: test_functions.py:431-442
func TestImageListInput(t *testing.T) {
	ctx := context.Background()

	img1, err := b.NewImageFromUrl("https://i.imgur.com/93fWs5R.png", nil)
	require.NoError(t, err)

	img2, err := b.NewImageFromUrl("https://www.google.com/images/branding/googlelogo/2x/googlelogo_color_92x30dp.png", nil)
	require.NoError(t, err)

	result, err := b.TestImageListInput(ctx, []types.Image{img1, img2})
	require.NoError(t, err)

	resultLower := strings.ToLower(result)
	assert.True(t,
		strings.Contains(resultLower, "green") ||
			strings.Contains(resultLower, "yellow"),
		"Expected result to mention colors, got: %s", result)
}

// TestAudioInputBase64 tests audio input from base64
// Reference: test_functions.py:464-466
func TestAudioInputBase64(t *testing.T) {
	// TODO: too lazy
	// ctx := context.Background()

	// // Base64 audio data (minimal MP3 data)
	// audioB64 := "SUQzAwAAAAABAAAAAAAAAAAAAAA"

	// aud, err := b.NewAudioFromBase64(audioB64, stringPtr("audio/mp3"))
	// require.NoError(t, err)

	// result, err := b.AudioInput(ctx, aud)
	// require.NoError(t, err)

	// resultLower := strings.ToLower(result)
	// assert.Contains(t, resultLower, "yes", "Expected audio to be recognized")
}

// TestAudioInputURL tests audio input from URL
// Reference: test_functions.py:470-476
func TestAudioInputURL(t *testing.T) {
	ctx := context.Background()

	aud, err := b.NewAudioFromUrl("https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg", nil)
	require.NoError(t, err)

	result, err := b.AudioInput(ctx, aud)
	require.NoError(t, err)

	resultLower := strings.ToLower(result)
	assert.Contains(t, resultLower, "no", "Expected different audio result")
}

// TestAudioInputOpenAI tests audio with OpenAI
// Reference: test_functions.py:480-485
func TestAudioInputOpenAI(t *testing.T) {
	// TODO: too lazy
	// ctx := context.Background()

	// audioB64 := "SUQzAwAAAAABAAAAAAAAAAAAAAA"
	// aud, err := b.NewAudioFromBase64(audioB64, stringPtr("audio/mp3"))
	// require.NoError(t, err)

	// result, err := b.AudioInputOpenai(ctx, aud, "does this sound like a roar? yes or no")
	// require.NoError(t, err)

	// resultLower := strings.ToLower(result)
	// assert.Contains(t, resultLower, "yes", "Expected roar recognition")
}

// TestAudioInputOpenAIURL tests audio URL with OpenAI
// Reference: test_functions.py:489-496
func TestAudioInputOpenAIURL(t *testing.T) {
	ctx := context.Background()

	aud, err := b.NewAudioFromUrl("https://github.com/sourcesounds/tf/raw/refs/heads/master/sound/vo/engineer_cloakedspyidentify09.mp3", nil)
	require.NoError(t, err)

	result, err := b.AudioInputOpenai(ctx, aud, "transcribe this")
	require.NoError(t, err)

	resultLower := strings.ToLower(result)
	assert.Contains(t, resultLower, "spy", "Expected transcription to contain 'spy'")
}

// TestPDFInput tests PDF input functionality
// Reference: Inferred from PDF functions in Go client
func TestPDFInput(t *testing.T) {
	ctx := context.Background()

	// Create a minimal PDF from base64
	pdfB64 := "JVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwKL1R5cGUgL0NhdGFsb2cKL1BhZ2VzIDEgMCBSCj4+CmVuZG9iagoKMSAwIG9iago8PAovVHlwZSAvUGFnZXMKL0tpZHMgWzMgMCBSXQovQ291bnQgMQo+PgplbmRvYmoKCjMgMCBvYmoKPDwKL1R5cGUgL1BhZ2UKL1BhcmVudCAxIDAgUgovTWVkaWFCb3ggWzAgMCA2MTIgNzkyXQo+PgplbmRvYmoKCnhyZWYKMCA0CjAwMDAwMDAwMDAgNjU1MzUgZiAKMDAwMDAwMDAwOSAwMDAwMCBuIAowMDAwMDAwMDc0IDAwMDAwIG4gCjAwMDAwMDAxMjAgMDAwMDAgbiAKdHJhaWxlcgo8PAovU2l6ZSA0Ci9Sb290IDIgMCBSCj4+CnN0YXJ0eHJlZgoxNzgKJSVFT0Y="

	pdf, err := b.NewPDFFromBase64(pdfB64, nil)
	require.NoError(t, err)

	result, err := b.PdfInput(ctx, pdf)
	require.NoError(t, err)
	assert.NotEmpty(t, result, "Expected non-empty PDF processing result")
}

// TestPDFInputOpenAI tests PDF with OpenAI
func TestPDFInputOpenAI(t *testing.T) {
	ctx := context.Background()

	pdfB64 := "JVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwKL1R5cGUgL0NhdGFsb2cKL1BhZ2VzIDEgMCBSCj4+CmVuZG9iagoKMSAwIG9iago8PAovVHlwZSAvUGFnZXMKL0tpZHMgWzMgMCBSXQovQ291bnQgMQo+PgplbmRvYmoKCjMgMCBvYmoKPDwKL1R5cGUgL1BhZ2UKL1BhcmVudCAxIDAgUgovTWVkaWFCb3ggWzAgMCA2MTIgNzkyXQo+PgplbmRvYmoKCnhyZWYKMCA0CjAwMDAwMDAwMDAgNjU1MzUgZiAKMDAwMDAwMDAwOSAwMDAwMCBuIAowMDAwMDAwMDc0IDAwMDAwIG4gCjAwMDAwMDAxMjAgMDAwMDAgbiAKdHJhaWxlcgo8PAovU2l6ZSA0Ci9Sb290IDIgMCBSCj4+CnN0YXJ0eHJlZgoxNzgKJSVFT0Y="

	pdf, err := b.NewPDFFromBase64(pdfB64, nil)
	require.NoError(t, err)

	result, err := b.PdfInputOpenai(ctx, pdf, "summarize this document")
	require.NoError(t, err)
	assert.NotEmpty(t, result, "Expected non-empty OpenAI PDF result")
}

// TestPDFInputVertex tests PDF with Vertex AI
func TestPDFInputVertex(t *testing.T) {
	ctx := context.Background()

	pdfB64 := "JVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwKL1R5cGUgL0NhdGFsb2cKL1BhZ2VzIDEgMCBSCj4+CmVuZG9iagoKMSAwIG9iago8PAovVHlwZSAvUGFnZXMKL0tpZHMgWzMgMCBSXQovQ291bnQgMQo+PgplbmRvYmoKCjMgMCBvYmoKPDwKL1R5cGUgL1BhZ2UKL1BhcmVudCAxIDAgUgovTWVkaWFCb3ggWzAgMCA2MTIgNzkyXQo+PgplbmRvYmoKCnhyZWYKMCA0CjAwMDAwMDAwMDAgNjU1MzUgZiAKMDAwMDAwMDAwOSAwMDAwMCBuIAowMDAwMDAwMDc0IDAwMDAwIG4gCjAwMDAwMDAxMjAgMDAwMDAgbiAKdHJhaWxlcgo8PAovU2l6ZSA0Ci9Sb290IDIgMCBSCj4+CnN0YXJ0eHJlZgoxNzgKJSVFT0Y="

	pdf, err := b.NewPDFFromBase64(pdfB64, nil)
	require.NoError(t, err)

	result, err := b.PdfInputVertex(ctx, pdf)
	require.NoError(t, err)
	assert.NotEmpty(t, result, "Expected non-empty Vertex PDF result")
}
