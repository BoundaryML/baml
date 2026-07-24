package generated_test

import (
	"context"
	"encoding/json"
	"strings"
	"testing"

	baml_sdk "baml.local/sdk/baml_sdk"
	baml "baml.local/sdk/baml_sdk/baml"
)

var (
	_ func(context.Context, string, ...baml_sdk.LoremExtractResumeBuildRequestStreamOption) (baml.HttpRequest, error) = baml_sdk.LoremExtractResumeBuildRequestStream
	_ func(baml.LlmClient) baml_sdk.LoremExtractResumeBuildRequestStreamOption                                        = baml_sdk.WithLoremExtractResumeBuildRequestStreamClient
)

func TestBuildRequestStreamSetsStreamingFlag(t *testing.T) {
	t.Setenv("OPENAI_API_KEY", "sk-build-stream-test")

	request, err := baml_sdk.LoremExtractResumeBuildRequestStream(
		context.Background(),
		"Stream this resume",
	)
	if err != nil {
		t.Fatalf("build streaming request: %v", err)
	}
	assertHeader(t, request.Headers, "authorization", "Bearer sk-build-stream-test")
	assertRequest(t, request, "api.openai.com", "Stream this resume")
	assertJSONBoolean(t, request.Body, "stream", true)
}

func TestBuildRequestStreamAcceptsExplicitClientOption(t *testing.T) {
	t.Setenv("ANTHROPIC_API_KEY", "sk-build-stream-client-test")
	client := baml.LlmClient{
		Name:       "anthropic/claude-3-5-sonnet-latest",
		ClientType: baml.LlmClientTypePrimitive,
		SubClients: []baml.LlmClient{},
	}

	request, err := baml_sdk.LoremExtractResumeBuildRequestStream(
		context.Background(),
		"Use an Anthropic stream",
		baml_sdk.WithLoremExtractResumeBuildRequestStreamClient(client),
	)
	if err != nil {
		t.Fatalf("build streaming request with client override: %v", err)
	}
	assertHeader(t, request.Headers, "x-api-key", "sk-build-stream-client-test")
	assertRequest(t, request, "api.anthropic.com", "Use an Anthropic stream")
	assertJSONBoolean(t, request.Body, "stream", true)
}

func TestBuildRequestStreamHonorsCancellation(t *testing.T) {
	t.Setenv("OPENAI_API_KEY", "sk-build-stream-cancelled")
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	_, err := baml_sdk.LoremExtractResumeBuildRequestStream(ctx, "cancel me")
	if err != ctx.Err() {
		t.Fatalf("build streaming request error = %v, want exact context error %v", err, ctx.Err())
	}
}

func TestBuildRequestStreamReturnsRuntimeErrorForInvalidClient(t *testing.T) {
	client := baml.LlmClient{
		Name:       "not-a-provider/not-a-model",
		ClientType: baml.LlmClientTypePrimitive,
		SubClients: []baml.LlmClient{},
	}

	_, err := baml_sdk.LoremExtractResumeBuildRequestStream(
		context.Background(),
		"invalid client",
		baml_sdk.WithLoremExtractResumeBuildRequestStreamClient(client),
	)
	if err == nil {
		t.Fatal("invalid streaming client unexpectedly succeeded")
	}
	if !strings.HasPrefix(err.Error(), "BAML ") || !strings.Contains(err.Error(), "user.lorem.ExtractResume$build_request_stream") {
		t.Fatalf("streaming request error lost kind or exact companion trace identity: %v", err)
	}
}

func assertJSONBoolean(t *testing.T, body string, key string, want bool) {
	t.Helper()
	var decoded map[string]any
	if err := json.Unmarshal([]byte(body), &decoded); err != nil {
		t.Fatalf("decode request body: %v", err)
	}
	got, ok := decoded[key].(bool)
	if !ok || got != want {
		t.Fatalf("request body %q = %#v, want %v in %s", key, decoded[key], want, body)
	}
}
