// Go smoke for the BEP phase 1 reference: the generated SDK drives the
// live Claude Code client, and `json`-typed BAML values cross the bridge
// as plain Go `any`.
//
// Run from this directory:
//
//	cargo build -p bridge_cffi   # once, from the workspace root
//	BAML_RUNTIME_PATH=../../target/debug/libbridge_cffi.dylib go test -v ./...
//
// The live test shells out to the authenticated `claude` CLI; skip it with
// `go test -short ./...`.
package planv2_test

import (
	"context"
	"strings"
	"sync"
	"testing"
	"time"

	baml_sdk "baml.local/planv2/baml_sdk"
	baml "baml.local/planv2/baml_sdk/baml"
	baml_go "github.com/boundaryml/baml-go"
)

// A BAML `json` value maps to Go `any` in both directions: functions
// returning `json` return `any` (objects decode as map[string]any, arrays
// as []any, numbers as int64/float64), and `json` parameters accept plain
// Go values.
func Test_json_maps_to_any(t *testing.T) {
	ctx := context.Background()

	// baml.json.parse: string -> json, surfaced as (any, error).
	parsed, err := baml.JsonParse(ctx, `{"type":"result","num_turns":3,"is_error":false}`)
	if err != nil {
		t.Fatalf("JsonParse: %v", err)
	}
	t.Logf("JsonParse returned dynamic type %T: %v", parsed, parsed)

	obj, ok := parsed.(map[string]any)
	if !ok {
		t.Fatalf("expected a json object to decode as map[string]any, got %T", parsed)
	}
	if obj["type"] != "result" {
		t.Fatalf("expected type=result, got %v", obj["type"])
	}
	if obj["num_turns"] != int64(3) {
		t.Fatalf("expected num_turns to decode as int64(3), got %T %v", obj["num_turns"], obj["num_turns"])
	}

	// baml.json.field reads one key of a json value supplied from Go.
	kind, err := baml.JsonField(ctx, parsed, "type")
	if err != nil {
		t.Fatalf("JsonField: %v", err)
	}
	if kind != "result" {
		t.Fatalf("expected field type to be result, got %v", kind)
	}

	// baml.json.stringify: json -> string, over a value built in Go.
	rendered, err := baml.JsonStringify(ctx, map[string]any{
		"outcome": map[string]any{
			"calls": []any{map[string]any{"id": "c1", "name": "search_flights"}},
		},
	})
	if err != nil {
		t.Fatalf("JsonStringify: %v", err)
	}
	if !strings.Contains(rendered, `"search_flights"`) {
		t.Fatalf("stringify lost content: %s", rendered)
	}

	// KNOWN LIMITATION (task filed): an object encoded from Go does not yet
	// match BAML's `map<string, json>` typed patterns, so `baml.json.path` /
	// `path_or` treat it as a non-object and fall back. When this assertion
	// starts failing, the bridge was fixed — move the demo to JsonPathOr.
	fallback, err := baml.JsonPathOr[string](ctx, parsed, ".type", "?")
	if err != nil {
		t.Fatalf("JsonPathOr: %v", err)
	}
	if fallback != "?" {
		t.Fatalf("Go-inbound json now matches map<string, json>: path_or returned %q — update this test and the filed task", fallback)
	}

	// A project function with a `json` parameter takes the Go value
	// directly: _log_cc_event(raw: json) logs one line per CLI event.
	fakeEvent := map[string]any{
		"type":    "system",
		"subtype": "init",
		"model":   "claude-haiku-4-5",
	}
	if _, err := baml_sdk.ClaudeCodeInternalLogCcEvent(ctx, fakeEvent); err != nil {
		t.Fatalf("ClaudeCodeInternalLogCcEvent: %v", err)
	}
}

// The live smoke: the whole agent loop — runner, journal, the outcome
// envelope, and the `claude` CLI as the client — driven from Go through
// the generated binding for live_claude_code().
func Test_live_claude_code(t *testing.T) {
	if testing.Short() {
		t.Skip("live: shells out to the authenticated claude CLI")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Minute)
	defer cancel()

	summary, err := baml_sdk.LiveClaudeCode(ctx)
	if err != nil {
		t.Fatalf("live_claude_code: %v", err)
	}
	t.Logf("summary: %s", summary)

	if !strings.Contains(summary, "flights=") {
		t.Fatalf("summary missing flights count: %s", summary)
	}
	if strings.Contains(summary, "tool_completed_events=0") {
		t.Fatalf("the BAML tool loop never completed a call: %s", summary)
	}
}

// The observed variant: a Go func is the runner's on_event observer. BAML
// calls back into Go for every journal event as it appends, with the event
// payload crossing as json -> any. This is how a host watches a run live
// even though BAML log.* output is not surfaced through the bridge.
func Test_live_claude_code_observed(t *testing.T) {
	if testing.Short() {
		t.Skip("live: shells out to the authenticated claude CLI")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Minute)
	defer cancel()

	var mu sync.Mutex
	var kinds []string
	summary, err := baml_sdk.LiveClaudeCodeObserved(ctx, func(kind string, payload any) baml_go.Null {
		mu.Lock()
		defer mu.Unlock()
		kinds = append(kinds, kind)
		t.Logf("event %-16s %v", kind, payload)
		return baml_go.Null{}
	})
	if err != nil {
		t.Fatalf("live_claude_code_observed: %v", err)
	}
	t.Logf("summary: %s", summary)

	mu.Lock()
	defer mu.Unlock()
	seen := make(map[string]int, len(kinds))
	for _, k := range kinds {
		seen[k]++
	}
	for _, required := range []string{"RunStarted", "AssistantMessage", "ToolRequested", "ToolCompleted", "Usage", "FinalProduced"} {
		if seen[required] == 0 {
			t.Errorf("no %s event reached the Go observer (saw %v)", required, seen)
		}
	}
	if strings.Contains(summary, "tool_completed_events=0") {
		t.Fatalf("the BAML tool loop never completed a call: %s", summary)
	}
}
