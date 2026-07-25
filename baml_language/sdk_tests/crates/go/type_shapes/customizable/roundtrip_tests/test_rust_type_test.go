package sdk_test

import (
	"context"
	"fmt"
	"sync"
	"testing"

	b "baml.local/sdk/baml_sdk"
	"baml.local/sdk/baml_sdk/baml"
)

func Test_opaque_rust_type_round_trips_and_remains_reusable(t *testing.T) {
	ctx := context.Background()
	response, err := b.GoCodegenRustTypeEdgesMakeOpaqueResponse(ctx, "owned by BAML")
	if err != nil {
		t.Fatal(err)
	}
	if response.StatusCode != 207 || response.Headers["x-baml-go"] != "opaque" {
		t.Fatalf("unexpected response metadata: %#v", response)
	}

	for iteration := 0; iteration < 3; iteration++ {
		text, err := b.GoCodegenRustTypeEdgesReadOpaqueResponse(ctx, response)
		if err != nil {
			t.Fatal(err)
		}
		if text != "owned by BAML" {
			t.Fatalf("unexpected body %q", text)
		}
		response, err = b.GoCodegenRustTypeEdgesRoundTripOpaqueResponse(ctx, response)
		if err != nil {
			t.Fatal(err)
		}
	}

	const workers = 12
	var group sync.WaitGroup
	errors := make(chan error, workers)
	for worker := 0; worker < workers; worker++ {
		group.Add(1)
		go func() {
			defer group.Done()
			text, err := b.GoCodegenRustTypeEdgesReadOpaqueResponse(ctx, response)
			if err != nil {
				errors <- err
				return
			}
			if text != "owned by BAML" {
				errors <- fmt.Errorf("unexpected concurrent body %q", text)
			}
		}()
	}
	group.Wait()
	close(errors)
	for err := range errors {
		t.Error(err)
	}
}

func Test_opaque_rust_type_nested_containers_classes_and_null(t *testing.T) {
	ctx := context.Background()
	first, err := b.GoCodegenRustTypeEdgesMakeOpaqueResponse(ctx, "first")
	if err != nil {
		t.Fatal(err)
	}
	second, err := b.GoCodegenRustTypeEdgesMakeOpaqueResponse(ctx, "second")
	if err != nil {
		t.Fatal(err)
	}

	responses, err := b.GoCodegenRustTypeEdgesRoundTripOpaqueResponses(ctx, []baml.HttpResponse{first, second})
	if err != nil {
		t.Fatal(err)
	}
	responseMap, err := b.GoCodegenRustTypeEdgesRoundTripOpaqueResponseMap(ctx, map[string]baml.HttpResponse{"first": first, "second": second})
	if err != nil {
		t.Fatal(err)
	}
	none, err := b.GoCodegenRustTypeEdgesRoundTripOptionalOpaqueResponse(ctx, nil)
	if err != nil || none != nil {
		t.Fatalf("optional null did not round-trip: value=%#v err=%v", none, err)
	}

	envelope := b.GoCodegenRustTypeEdgesResponseEnvelope{
		Response:         first,
		OptionalResponse: &second,
		Responses:        responses,
		ResponsesByName:  responseMap,
	}
	decoded, err := b.GoCodegenRustTypeEdgesRoundTripResponseEnvelope(ctx, envelope)
	if err != nil {
		t.Fatal(err)
	}
	for name, item := range map[string]struct {
		response baml.HttpResponse
		expected string
	}{
		"response": {decoded.Response, "first"},
		"optional": {*decoded.OptionalResponse, "second"},
		"list":     {decoded.Responses[1], "second"},
		"map":      {decoded.ResponsesByName["first"], "first"},
	} {
		text, err := b.GoCodegenRustTypeEdgesReadOpaqueResponse(ctx, item.response)
		if err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		if text != item.expected {
			t.Fatalf("%s: got body %q, want %q", name, text, item.expected)
		}
	}
}

func Test_opaque_rust_type_default_and_host_callback_positions(t *testing.T) {
	ctx := context.Background()
	defaulted, err := b.GoCodegenRustTypeEdgesDefaultedOpaqueResponse(ctx)
	if err != nil {
		t.Fatal(err)
	}
	text, err := b.GoCodegenRustTypeEdgesReadOpaqueResponse(ctx, defaulted)
	if err != nil || text != "default body" {
		t.Fatalf("defaulted response: body=%q err=%v", text, err)
	}

	original, err := b.GoCodegenRustTypeEdgesMakeOpaqueResponse(ctx, "callback body")
	if err != nil {
		t.Fatal(err)
	}
	called := 0
	returned, err := b.GoCodegenRustTypeEdgesInvokeOpaqueResponseCallback(
		ctx,
		func(response baml.HttpResponse) baml.HttpResponse {
			called++
			return response
		},
		original,
	)
	if err != nil {
		t.Fatal(err)
	}
	if called != 1 {
		t.Fatalf("callback called %d times, want 1", called)
	}
	text, err = b.GoCodegenRustTypeEdgesReadOpaqueResponse(ctx, returned)
	if err != nil || text != "callback body" {
		t.Fatalf("callback response: body=%q err=%v", text, err)
	}
	// Both the callback result and the original remain independently reusable.
	text, err = b.GoCodegenRustTypeEdgesReadOpaqueResponse(ctx, original)
	if err != nil || text != "callback body" {
		t.Fatalf("original after callback: body=%q err=%v", text, err)
	}
}
