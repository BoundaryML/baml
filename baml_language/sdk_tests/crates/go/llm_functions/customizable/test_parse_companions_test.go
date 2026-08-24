package generated_test

import (
	"context"
	"os"
	"strings"
	"testing"

	baml_sdk "baml.local/sdk/baml_sdk"
)

// `$parse` is a compiler-synthesized ordinary callable: JSON replaces the
// parent's prompt arguments and the parent return type is preserved. It takes
// no client — parsing is local and network-free, so the single-path companion
// dropped the client override the legacy one carried.
var (
	_ func(context.Context, string) (baml_sdk.LoremResume, error)        = baml_sdk.LoremExtractResumeParse
	_ func(context.Context, string) (baml_sdk.IpsumSentiment, error)     = baml_sdk.IpsumClassifySentimentParse
)

func Test_parse_companion_returns_typed_class_and_fills_missing_nullable_field(t *testing.T) {
	withoutProviderCredentials(t)

	got, err := baml_sdk.LoremExtractResumeParse(context.Background(), `{"name":"Ada"}`)
	if err != nil {
		t.Fatalf("parse resume: %v", err)
	}
	if got.Name != "Ada" || got.Email != nil {
		t.Fatalf("parsed resume = %#v, want name Ada and nil email", got)
	}
}

func Test_parse_companion_returns_closed_enum(t *testing.T) {
	withoutProviderCredentials(t)

	got, err := baml_sdk.IpsumClassifySentimentParse(context.Background(), `"POSITIVE"`)
	if err != nil {
		t.Fatalf("parse sentiment: %v", err)
	}
	if got != baml_sdk.IpsumSentimentPOSITIVE {
		t.Fatalf("parsed sentiment = %q, want POSITIVE", got)
	}
}

func Test_parse_companion_honors_cancellation(t *testing.T) {
	withoutProviderCredentials(t)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	_, err := baml_sdk.LoremExtractResumeParse(ctx, `{"name":"ignored"}`)
	if err != ctx.Err() {
		t.Fatalf("parse error = %v, want exact context error %v", err, ctx.Err())
	}
}

func Test_parse_companion_returns_runtime_error_for_invalid_output(t *testing.T) {
	withoutProviderCredentials(t)

	_, err := baml_sdk.LoremExtractResumeParse(context.Background(), `not a resume`)
	if err == nil {
		t.Fatal("invalid parse unexpectedly succeeded")
	}
	if !strings.HasPrefix(err.Error(), "BAML ") || !strings.Contains(err.Error(), "user.lorem.ExtractResume$parse") {
		t.Fatalf("parse error lost kind or exact companion trace identity: %v", err)
	}
}

// Parse is an offline operation: it must not accidentally inherit the
// credential requirements of calling or building a request for the parent LLM
// function. These tests intentionally remain non-parallel because environment
// variables are process-global.
func withoutProviderCredentials(t *testing.T) {
	t.Helper()
	for _, name := range []string{"OPENAI_API_KEY", "ANTHROPIC_API_KEY"} {
		name := name
		value, wasPresent := os.LookupEnv(name)
		if err := os.Unsetenv(name); err != nil {
			t.Fatalf("unset %s: %v", name, err)
		}
		t.Cleanup(func() {
			if wasPresent {
				if err := os.Setenv(name, value); err != nil {
					t.Errorf("restore %s: %v", name, err)
				}
				return
			}
			if err := os.Unsetenv(name); err != nil {
				t.Errorf("restore absent %s: %v", name, err)
			}
		})
	}
}
