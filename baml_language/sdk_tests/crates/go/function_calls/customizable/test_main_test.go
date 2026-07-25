package sdk_test

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
)

// Direct port of Python test_hello_world_returns_literal.
func Test_hello_world_returns_literal(t *testing.T) {
	got, err := baml_sdk.HelloWorld(context.Background())
	if err != nil || got != "hello world" {
		t.Fatalf("HelloWorld() = %q, %v", got, err)
	}
}

// Direct port of Python test_single_required_arg_round_trips.
func Test_single_required_arg_round_trips(t *testing.T) {
	got, err := baml_sdk.SingleRequiredArg(context.Background(), "hi")
	if err != nil || got != "hi" {
		t.Fatalf("SingleRequiredArg() = %q, %v", got, err)
	}
}
