package sdk_test

import (
	"context"
	"encoding/json"
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"

	"baml.local/sdk/baml_sdk"
	"github.com/boundaryml/baml-go"
)

var (
	_ func(baml_sdk.MethodSelfEdgesMethodSelfEdges, context.Context, string) (string, error)      = baml_sdk.MethodSelfEdgesMethodSelfEdges.ThrowError
	_ func(baml_sdk.MethodSelfEdgesMethodSelfEdges, context.Context, string) error                = baml_sdk.MethodSelfEdgesMethodSelfEdges.Panic
	_ func(baml_sdk.MethodSelfEdgesMethodSelfEdges, context.Context, int64) (baml_go.Null, error) = baml_sdk.MethodSelfEdgesMethodSelfEdges.SleepMs
	_ func(
		baml_sdk.MethodSelfEdgesMethodSelfEdges,
		context.Context,
		baml_go.Image,
		...baml_sdk.MethodSelfEdgesMethodSelfEdgesRoundTripImageOption,
	) (baml_go.Image, error) = baml_sdk.MethodSelfEdgesMethodSelfEdges.RoundTripImage
	_ func(string) baml_sdk.MethodSelfEdgesMethodSelfEdgesRoundTripImageOption = baml_sdk.WithMethodSelfEdgesMethodSelfEdgesRoundTripImageExpectedReceiver
)

// Direct synchronous Go port of the canonical Python/TypeScript/Rust
// instance-method round trips. Go has one context-aware synchronous surface.
func TestInstanceMethodsOnClassesRoundTrip(t *testing.T) {
	ctx := context.Background()
	greeter := baml_sdk.MethodsOnClassesGreeter{Name: "hopper"}
	if got, err := greeter.Who(ctx); err != nil || got != "hopper" {
		t.Fatalf("Who() = %q, %v", got, err)
	}
	if got, err := greeter.Greet(ctx, "hello"); err != nil || got != "hello" {
		t.Fatalf("Greet() = %q, %v", got, err)
	}
}

// Direct port of the instance-method half of Python test_opt_box_method_matrix.
// The static OptBox.make call remains deferred until Go's static-method public
// shape is decided; constructing the same value proves receiver/default wiring.
func TestInstanceMethodOptionalArguments(t *testing.T) {
	ctx := context.Background()
	box := baml_sdk.OptBox{Base: 10}
	want := []*int64{int64Pointer(10), int64Pointer(1), int64Pointer(5)}
	if got, err := box.Probe(ctx, 1); err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("Probe(default) = %#v, %v, want %#v", got, err, want)
	}
	want = []*int64{int64Pointer(10), int64Pointer(1), int64Pointer(8)}
	if got, err := box.Probe(ctx, 1, baml_sdk.WithOptBoxProbeOpt1(int64Pointer(8))); err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("Probe(override) = %#v, %v, want %#v", got, err, want)
	}
}

func TestMethodSelfAllSupportedPositionsRoundTrip(t *testing.T) {
	ctx := context.Background()
	var value baml_sdk.MethodSelfEdgesMethodSelfEdges
	if err := json.Unmarshal([]byte(`{"round_trip":"edge"}`), &value); err != nil {
		t.Fatal(err)
	}

	if got, err := value.Clone(ctx); err != nil || !reflect.DeepEqual(got, value) {
		t.Fatalf("Clone() = %#v, %v, want %#v", got, err, value)
	}
	if got, err := value.Nullable(ctx); err != nil || got != nil {
		t.Fatalf("Nullable(default) = %#v, %v, want nil", got, err)
	}
	if got, err := value.Nullable(ctx, baml_sdk.WithMethodSelfEdgesMethodSelfEdgesNullableValue(&value)); err != nil || got == nil || !reflect.DeepEqual(*got, value) {
		t.Fatalf("Nullable(value) = %#v, %v, want %#v", got, err, value)
	}

	list := []baml_sdk.MethodSelfEdgesMethodSelfEdges{value}
	if got, err := value.List(ctx, list); err != nil || !reflect.DeepEqual(got, list) {
		t.Fatalf("List() = %#v, %v, want %#v", got, err, list)
	}
	values := map[string]baml_sdk.MethodSelfEdgesMethodSelfEdges{"value": value}
	if got, err := value.Map(ctx, values); err != nil || !reflect.DeepEqual(got, values) {
		t.Fatalf("Map() = %#v, %v, want %#v", got, err, values)
	}
	if got, err := value.Alias(ctx, value); err != nil || !reflect.DeepEqual(got, value) {
		t.Fatalf("Alias() = %#v, %v, want %#v", got, err, value)
	}

	union := baml_sdk.NewStringOrMethodSelfEdgesMethodSelfEdgesFromMethodSelfEdgesMethodSelfEdges(value)
	gotUnion, err := value.Union(ctx, union)
	if err != nil {
		t.Fatal(err)
	}
	gotValue, ok := gotUnion.AsMethodSelfEdgesMethodSelfEdges()
	if !ok || !reflect.DeepEqual(gotValue, value) {
		t.Fatalf("Union(Self) = %#v, %v", gotValue, ok)
	}
	textUnion := baml_sdk.NewStringOrMethodSelfEdgesMethodSelfEdgesFromString("text")
	gotUnion, err = value.Union(ctx, textUnion)
	if err != nil {
		t.Fatal(err)
	}
	if gotText, ok := gotUnion.AsString(); !ok || gotText != "text" {
		t.Fatalf("Union(string) = %q, %v", gotText, ok)
	}
}

func TestEmptyClassSelfRoundTrip(t *testing.T) {
	value := baml_sdk.MethodSelfEdgesEmptyMethodSelf{}
	got, err := value.Identity(context.Background())
	if err != nil || got != value {
		t.Fatalf("Identity() = %#v, %v, want %#v", got, err, value)
	}
}

func TestMethodGeneratedNameCollisionsStayCallable(t *testing.T) {
	ctx := context.Background()
	var value baml_sdk.MethodSelfEdgesMethodSelfEdges
	if err := json.Unmarshal([]byte(`{"round_trip":"edge"}`), &value); err != nil {
		t.Fatal(err)
	}
	if value.BAMLClassName() != "user.method_self_edges.MethodSelfEdges" {
		t.Fatalf("metadata name = %q", value.BAMLClassName())
	}
	if got, err := value.BAMLClassName_(ctx); err != nil || got != "edge" {
		t.Fatalf("BAMLClassName_() = %q, %v", got, err)
	}
	if got, err := value.RoundTrip_fb92160d(ctx); err != nil || got != "edge" {
		t.Fatalf("projected collision method = %q, %v", got, err)
	}
	want := []string{"ctx", "err", "result", "zero", "bootstrap", "receiver"}
	got, err := value.ProtectedLocals(ctx, want[0], want[1], want[2], want[3], want[4], want[5])
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("ProtectedLocals() = %#v, %v, want %#v", got, err, want)
	}
}

func TestInstanceMethodThrowPreservesCurrentGoErrorContract(t *testing.T) {
	ctx := context.Background()
	var value baml_sdk.MethodSelfEdgesMethodSelfEdges
	if err := json.Unmarshal([]byte(`{"round_trip":"edge"}`), &value); err != nil {
		t.Fatal(err)
	}
	got, err := value.ThrowError(ctx, "broken")
	if got != "" || err == nil {
		t.Fatalf("ThrowError() = %q, %v", got, err)
	}
	const wantFirstLine = "BAML error: user.method_self_edges.MethodSelfError: edge:broken"
	lines := strings.Split(err.Error(), "\n")
	if lines[0] != wantFirstLine {
		t.Fatalf("throw kind/FQN/message = %q, want %q", lines[0], wantFirstLine)
	}
	const wantTraceSuffix = ", in user.method_self_edges.MethodSelfEdges.throw_error"
	if len(lines) != 2 || !strings.HasPrefix(lines[1], `    File "`) || !strings.HasSuffix(lines[1], wantTraceSuffix) {
		t.Fatalf("throw trace = %#v", lines[1:])
	}
}

func TestInstanceNeverMethodHasErrorOnlySignatureAndReturnsPanic(t *testing.T) {
	ctx := context.Background()
	var value baml_sdk.MethodSelfEdgesMethodSelfEdges
	if err := json.Unmarshal([]byte(`{"round_trip":"edge"}`), &value); err != nil {
		t.Fatal(err)
	}
	var panicMethod func(context.Context, string) error = value.Panic
	err := panicMethod(ctx, "boom")
	if err == nil {
		t.Fatal("Panic() returned nil")
	}
	const wantFirstLine = "BAML panic: baml.panics.UserPanic: edge:boom"
	lines := strings.Split(err.Error(), "\n")
	if lines[0] != wantFirstLine {
		t.Fatalf("panic kind/FQN/message = %q, want %q", lines[0], wantFirstLine)
	}
	const wantTraceSuffix = ", in user.method_self_edges.MethodSelfEdges.panic"
	if len(lines) != 2 || !strings.HasPrefix(lines[1], `    File "`) || !strings.HasSuffix(lines[1], wantTraceSuffix) {
		t.Fatalf("panic trace = %#v", lines[1:])
	}
}

func TestInstanceMethodCancellationReturnsExactContextError(t *testing.T) {
	var value baml_sdk.MethodSelfEdgesMethodSelfEdges
	ctx, cancel := context.WithTimeout(context.Background(), 150*time.Millisecond)
	defer cancel()
	start := time.Now()
	_, err := value.SleepMs(ctx, 2000)
	if err != ctx.Err() {
		t.Fatalf("SleepMs error identity = %v, want exact ctx.Err() %v", err, ctx.Err())
	}
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("SleepMs error = %v, want deadline exceeded", err)
	}
	if elapsed := time.Since(start); elapsed >= time.Second {
		t.Fatalf("SleepMs cancellation took %s", elapsed)
	}
}

func TestInstanceMethodMediaReceiverDefaultAndOwnershipRoundTrip(t *testing.T) {
	ctx := context.Background()
	var value baml_sdk.MethodSelfEdgesMethodSelfEdges
	if err := json.Unmarshal([]byte(`{"round_trip":"edge"}`), &value); err != nil {
		t.Fatal(err)
	}
	const mediaURL = "https://example.com/method.png"
	mime := "image/png"
	image, err := baml_go.NewImageFromUrl(mediaURL, &mime)
	if err != nil {
		t.Fatal(err)
	}
	got, err := value.RoundTripImage(ctx, image)
	if err != nil {
		t.Fatal(err)
	}
	url, err := got.Url()
	if err != nil || url == nil || *url != mediaURL {
		t.Fatalf("round-tripped URL = %#v, %v", url, err)
	}
	gotMime, err := got.MimeType()
	if err != nil || gotMime == nil || *gotMime != mime {
		t.Fatalf("round-tripped MIME = %#v, %v", gotMime, err)
	}
	// Reusing both values proves the constructor owner and the independently
	// cloned outbound owner remain valid after the call transaction releases
	// its temporary inbound clone.
	if _, err := value.RoundTripImage(ctx, image); err != nil {
		t.Fatalf("reuse original media: %v", err)
	}
	if _, err := value.RoundTripImage(ctx, got); err != nil {
		t.Fatalf("reuse returned media: %v", err)
	}
	_, err = value.RoundTripImage(
		ctx,
		image,
		baml_sdk.WithMethodSelfEdgesMethodSelfEdgesRoundTripImageExpectedReceiver("wrong"),
	)
	if err == nil || !strings.HasPrefix(err.Error(), "BAML panic: baml.panics.UserPanic: wrong method receiver") {
		t.Fatalf("receiver/default override panic = %v", err)
	}
}

func int64Pointer(value int64) *int64 { return &value }
