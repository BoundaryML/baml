package generated_test

import (
	"context"
	"encoding/json"
	"strings"
	"testing"

	baml_sdk "baml.local/sdk/baml_sdk"
	baml "baml.local/sdk/baml_sdk/baml"
)

// These behavioral checks mirror the Python llm_functions build-request
// coverage. They exercise compiler-synthesized companions through generated
// Go, then inspect the structural baml.http.Request returned by the bridge.

var (
	_ func(context.Context, string, ...baml_sdk.LoremExtractResumeBuildRequestOption) (baml.HttpRequest, error) = baml_sdk.LoremExtractResumeBuildRequest
	_ func(baml.LlmClient) baml_sdk.LoremExtractResumeBuildRequestOption                                        = baml_sdk.WithLoremExtractResumeBuildRequestClient
)

func Test_extract_resume_build_request_includes_open_aiapi_key(t *testing.T) {
	t.Setenv("OPENAI_API_KEY", "sk-openai-shorthand-test")

	request, err := baml_sdk.LoremExtractResumeBuildRequest(
		context.Background(),
		"Some resume text",
	)
	if err != nil {
		t.Fatalf("build request: %v", err)
	}

	assertHeader(t, request.Headers, "authorization", "Bearer sk-openai-shorthand-test")
	assertRequest(t, request, "api.openai.com", "Some resume text")
}

func Test_streaming_extract_build_request_includes_open_aiapi_key(t *testing.T) {
	t.Setenv("OPENAI_API_KEY", "sk-openai-responses-test")

	request, err := baml_sdk.LoremStreamingExtractBuildRequest(
		context.Background(),
		"Some text to summarize",
	)
	if err != nil {
		t.Fatalf("build request: %v", err)
	}

	assertHeader(t, request.Headers, "authorization", "Bearer sk-openai-responses-test")
	assertRequest(t, request, "/responses", "Some text to summarize")
}

func Test_classify_sentiment_build_request_includes_anthropic_api_key(t *testing.T) {
	t.Setenv("ANTHROPIC_API_KEY", "sk-ant-shorthand-test")

	request, err := baml_sdk.IpsumClassifySentimentBuildRequest(
		context.Background(),
		"I love this!",
	)
	if err != nil {
		t.Fatalf("build request: %v", err)
	}

	assertHeader(t, request.Headers, "x-api-key", "sk-ant-shorthand-test")
	assertRequest(t, request, "api.anthropic.com", "I love this!")
}

func Test_build_request_accepts_explicit_client_option(t *testing.T) {
	t.Setenv("ANTHROPIC_API_KEY", "sk-ant-explicit-client")
	client := baml.LlmClient{
		Name:       "anthropic/claude-3-5-sonnet-latest",
		ClientType: baml.LlmClientTypePrimitive,
		SubClients: []baml.LlmClient{},
	}

	request, err := baml_sdk.LoremExtractResumeBuildRequest(
		context.Background(),
		"Use the explicit client",
		baml_sdk.WithLoremExtractResumeBuildRequestClient(client),
	)
	if err != nil {
		t.Fatalf("build request with explicit client: %v", err)
	}

	assertHeader(t, request.Headers, "x-api-key", "sk-ant-explicit-client")
	assertRequest(t, request, "api.anthropic.com", "Use the explicit client")
}

func Test_build_request_honors_cancellation(t *testing.T) {
	t.Setenv("OPENAI_API_KEY", "sk-cancelled")
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	_, err := baml_sdk.LoremExtractResumeBuildRequest(ctx, "cancel me")
	if err != ctx.Err() {
		t.Fatalf("build request error = %v, want exact context error %v", err, ctx.Err())
	}
}

func assertHeader(t *testing.T, headers map[string]string, name string, want string) {
	t.Helper()
	for key, value := range headers {
		if strings.EqualFold(key, name) {
			if value != want {
				t.Fatalf("header %q = %q, want %q", name, value, want)
			}
			return
		}
	}
	t.Fatalf("missing header %q in %#v", name, headers)
}

func assertRequest(t *testing.T, request baml.HttpRequest, urlFragment string, bodyFragment string) {
	t.Helper()
	if request.Method != "POST" {
		t.Fatalf("request method = %q, want POST", request.Method)
	}
	if !strings.Contains(request.Url, urlFragment) {
		t.Fatalf("request URL %q does not contain %q", request.Url, urlFragment)
	}
	if !json.Valid([]byte(request.Body)) {
		t.Fatalf("request body is not valid JSON: %q", request.Body)
	}
	if !strings.Contains(request.Body, bodyFragment) {
		t.Fatalf("request body does not contain %q: %s", bodyFragment, request.Body)
	}
	assertHeader(t, request.Headers, "content-type", "application/json")
}
