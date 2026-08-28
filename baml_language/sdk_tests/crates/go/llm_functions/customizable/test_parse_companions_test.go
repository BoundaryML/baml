package generated_test

import (
	"context"
	"os"
	"strings"
	"testing"

	baml_sdk "baml.local/sdk/baml_sdk"
	ai "baml.local/sdk/baml_sdk/packages/ai"
	baml_go "github.com/boundaryml/baml-go"
)

// Parsing is a method on the FunctionSpec returned by the authored function's
// Spec projection. There is no synthetic `$parse` function binding.

var (
	_ func(context.Context, string) (baml_go.FunctionSpec[baml_sdk.LoremResume], error)                                                                    = baml_sdk.LoremExtractResumeSpec
	_ func(context.Context, string, ...baml_sdk.LoremExtractResumeStreamOption) (baml_go.Stream[*baml_sdk.LoremResumeStream, baml_sdk.LoremResume], error) = baml_sdk.LoremExtractResumeStream
)

func Test_flat_stream_controls_are_typed_options(t *testing.T) {
	client := baml_sdk.LoremExtractResumeStreamClient(nil)
	onEvent := baml_sdk.LoremExtractResumeStreamOnEvent(func(baml_go.Value) {})
	_ = []baml_sdk.LoremExtractResumeStreamOption{client, onEvent}
}

func Test_legacy_function_companion_wire_bindings_are_absent(t *testing.T) {
	source, err := os.ReadFile("baml_sdk/functions.go")
	if err != nil {
		t.Fatalf("read generated functions: %v", err)
	}
	for _, oldFQN := range []string{
		"user.lorem.ExtractResume$spec",
		"user.lorem.ExtractResume$stream",
		"user.lorem.ExtractResume$parse",
		"user.lorem.ExtractResume$render_prompt",
		"user.lorem.ExtractResume$build_request",
	} {
		if strings.Contains(string(source), oldFQN) {
			t.Errorf("legacy companion wire binding survived: %s", oldFQN)
		}
	}
}

func Test_function_spec_prompt_uses_generated_ai_prompt_type_and_is_reusable(t *testing.T) {
	withoutProviderCredentials(t)

	ctx := context.Background()
	spec, err := baml_sdk.LoremExtractResumeSpec(ctx, "Ada Lovelace")
	if err != nil {
		t.Fatalf("build resume spec: %v", err)
	}
	prompt, err := spec.Prompt(ctx)
	if err != nil {
		t.Fatalf("render prompt: %v", err)
	}
	var exposed ai.Prompt = prompt
	firstText, err := exposed.Text(ctx)
	if err != nil {
		t.Fatalf("first prompt text: %v", err)
	}
	secondText, err := exposed.Text(ctx)
	if err != nil {
		t.Fatalf("second prompt text: %v", err)
	}
	if firstText == "" || secondText != firstText {
		t.Fatalf("repeated prompt text = %q then %q", firstText, secondText)
	}
	firstMessages, err := exposed.Messages(ctx)
	if err != nil {
		t.Fatalf("first prompt messages: %v", err)
	}
	secondMessages, err := exposed.Messages(ctx)
	if err != nil {
		t.Fatalf("second prompt messages: %v", err)
	}
	if len(firstMessages) == 0 || len(secondMessages) != len(firstMessages) {
		t.Fatalf("repeated prompt message counts = %d then %d", len(firstMessages), len(secondMessages))
	}
}

func Test_function_spec_parse_returns_typed_class_and_fills_missing_nullable_field(t *testing.T) {
	withoutProviderCredentials(t)

	ctx := context.Background()
	spec, err := baml_sdk.LoremExtractResumeSpec(ctx, "ignored")
	if err != nil {
		t.Fatalf("build resume spec: %v", err)
	}
	got, err := spec.Parse(ctx, `{"name":"Ada"}`)
	if err != nil {
		t.Fatalf("parse resume: %v", err)
	}
	if got.Name != "Ada" || got.Email != nil {
		t.Fatalf("parsed resume = %#v, want name Ada and nil email", got)
	}
}

func Test_function_spec_parse_returns_closed_enum(t *testing.T) {
	withoutProviderCredentials(t)

	ctx := context.Background()
	spec, err := baml_sdk.IpsumClassifySentimentSpec(ctx, "ignored")
	if err != nil {
		t.Fatalf("build sentiment spec: %v", err)
	}
	got, err := spec.Parse(ctx, `"POSITIVE"`)
	if err != nil {
		t.Fatalf("parse sentiment: %v", err)
	}
	if got != baml_sdk.IpsumSentimentPOSITIVE {
		t.Fatalf("parsed sentiment = %q, want POSITIVE", got)
	}
}

func Test_spec_projection_honors_cancellation(t *testing.T) {
	withoutProviderCredentials(t)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	_, err := baml_sdk.LoremExtractResumeSpec(ctx, "ignored")
	if err != ctx.Err() {
		t.Fatalf("parse error = %v, want exact context error %v", err, ctx.Err())
	}
}

func Test_function_spec_parse_returns_runtime_error_for_invalid_output(t *testing.T) {
	withoutProviderCredentials(t)

	ctx := context.Background()
	spec, err := baml_sdk.LoremExtractResumeSpec(ctx, "ignored")
	if err != nil {
		t.Fatalf("build resume spec: %v", err)
	}
	_, err = spec.Parse(ctx, `not a resume`)
	if err == nil {
		t.Fatal("invalid parse unexpectedly succeeded")
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
