package sdk_test

import (
	"context"
	"errors"
	"testing"
	"time"

	"baml.local/sdk/baml_sdk"
)

// Direct synchronous Go counterparts to Python test_cancellation.py. Go uses
// context cancellation rather than a separate generated async API.
func Test_sync_call_returns_null(t *testing.T) {
	if _, err := baml_sdk.ThrowsTestSleepMs(context.Background(), 1); err != nil {
		t.Fatal(err)
	}
}

func Test_sync_cancel_via_context(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	timer := time.AfterFunc(50*time.Millisecond, cancel)
	defer timer.Stop()
	start := time.Now()
	_, err := baml_sdk.ThrowsTestSleepMs(ctx, 2000)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("cancellation error = %v", err)
	}
	if elapsed := time.Since(start); elapsed >= 500*time.Millisecond {
		t.Fatalf("cancellation took %s", elapsed)
	}
}

// Python async cancellation variants are N/A; Go cancellation is context-based.
