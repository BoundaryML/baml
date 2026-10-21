package baml_go

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/boundaryml/baml-go/internal/cffi"
	"google.golang.org/protobuf/proto"
)

var _ = map[Input]struct{}{}

type inputHandleLifecycle struct {
	mu       sync.Mutex
	next     uint64
	cloned   []uint64
	released []uint64
	failAt   int
}

func installInputHandleLifecycle(t *testing.T) *inputHandleLifecycle {
	t.Helper()
	lifecycle := &inputHandleLifecycle{next: 1000}
	previousClone := cloneInboundHandle
	previousMediaAccess := accessInboundMedia
	previousRelease := releaseInboundHandle
	cloneInboundHandle = func(source uint64) (uint64, error) {
		lifecycle.mu.Lock()
		defer lifecycle.mu.Unlock()
		lifecycle.cloned = append(lifecycle.cloned, source)
		if lifecycle.failAt != 0 && len(lifecycle.cloned) == lifecycle.failAt {
			return 0, errors.New("injected clone failure")
		}
		lifecycle.next++
		return lifecycle.next, nil
	}
	releaseInboundHandle = func(key uint64) {
		lifecycle.mu.Lock()
		defer lifecycle.mu.Unlock()
		lifecycle.released = append(lifecycle.released, key)
	}
	accessInboundMedia = func(operation mediaAccessor, key uint64, handleType cffi.BamlHandleType) (*string, error) {
		if key != 77 || handleType != cffi.BamlHandleType_ADT_MEDIA_IMAGE {
			return nil, errors.New("unexpected media handle")
		}
		var value string
		switch operation {
		case mediaURL:
			value = "https://example.test/image.png"
		case mediaMIMEType:
			value = "image/png"
		default:
			return nil, nil
		}
		return &value, nil
	}
	t.Cleanup(func() {
		cloneInboundHandle = previousClone
		accessInboundMedia = previousMediaAccess
		releaseInboundHandle = previousRelease
	})
	return lifecycle
}

func (lifecycle *inputHandleLifecycle) snapshot() (cloned, released []uint64) {
	lifecycle.mu.Lock()
	defer lifecycle.mu.Unlock()
	return append([]uint64(nil), lifecycle.cloned...), append([]uint64(nil), lifecycle.released...)
}

func fakeImageInput() Input {
	return ImageInput(Image{media: mediaValue{
		handle: &mediaHandle{key: 77, handleType: cffi.BamlHandleType_ADT_MEDIA_IMAGE},
		kind:   mediaKindImage,
	}})
}

func fakeRustTypeInput() Input {
	return RustTypeInput(RustType{handle: &rustTypeHandle{key: 77}})
}

func TestMediaInputDoesNotCloneBeforeTransactionalEncoding(t *testing.T) {
	lifecycle := installInputHandleLifecycle(t)
	input := fakeImageInput()
	if input.err != nil || input.deferred == nil {
		t.Fatalf("media input = %#v, want deferred encoder", input)
	}
	if cloned, released := lifecycle.snapshot(); len(cloned) != 0 || len(released) != 0 {
		t.Fatalf("constructing Input cloned/released handles: %v / %v", cloned, released)
	}
}

func TestCallPreDispatchFailuresNeverCloneMediaHandles(t *testing.T) {
	lifecycle := installInputHandleLifecycle(t)
	input := fakeImageInput()

	cancelled, cancel := context.WithCancel(context.Background())
	cancel()
	tests := []struct {
		name     string
		ctx      context.Context
		function string
	}{
		{name: "nil context", ctx: nil, function: "user.media.round_trip_image"},
		{name: "already cancelled", ctx: cancelled, function: "user.media.round_trip_image"},
		{name: "NUL function", ctx: context.Background(), function: "user.media.\x00bad"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if _, err := Call(test.ctx, test.function, map[string]Input{"x": input}); err == nil {
				t.Fatal("Call unexpectedly succeeded")
			}
		})
	}
	if cloned, released := lifecycle.snapshot(); len(cloned) != 0 || len(released) != 0 {
		t.Fatalf("pre-dispatch failure cloned/released handles: %v / %v", cloned, released)
	}
}

func TestCallRuntimeAcquisitionFailureNeverClonesMediaHandles(t *testing.T) {
	lifecycle := installInputHandleLifecycle(t)
	blocked := newNativeRuntimeState()
	<-blocked.initialization
	previousRuntime := nativeRuntime
	nativeRuntime = blocked
	t.Cleanup(func() {
		blocked.release()
		nativeRuntime = previousRuntime
	})

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Millisecond)
	defer cancel()
	if _, err := Call(ctx, "user.media.round_trip_image", map[string]Input{"x": fakeImageInput()}); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("Call error = %v, want deadline exceeded", err)
	}
	if cloned, released := lifecycle.snapshot(); len(cloned) != 0 || len(released) != 0 {
		t.Fatalf("runtime acquisition failure cloned/released handles: %v / %v", cloned, released)
	}
}

func TestTransactionalEncodingRollsBackTopLevelAndNestedPartialFailures(t *testing.T) {
	tests := []struct {
		name  string
		input Input
	}{
		{
			name:  "list",
			input: List([]Input{fakeImageInput(), InvalidInput("bad list sibling")}, func(value Input) Input { return value }),
		},
		{
			name: "map",
			input: Map(map[string]Input{
				"a_media": fakeImageInput(),
				"z_bad":   InvalidInput("bad map sibling"),
			}, func(value Input) Input { return value }),
		},
		{
			name: "class",
			input: Class("user.MediaBox", map[string]Input{
				"a_media": fakeImageInput(),
				"z_bad":   InvalidInput("bad class sibling"),
			}),
		},
		{
			name: "union payload",
			input: SelectedUnionInput(
				List([]Input{fakeImageInput(), InvalidInput("bad union payload sibling")}, func(value Input) Input { return value }),
				UnionBAMLType(ListBAMLType(ImageBAMLType()), PrimitiveBAMLType(StringType)),
				ListBAMLType(ImageBAMLType()),
			),
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			lifecycle := installInputHandleLifecycle(t)
			if _, transaction, err := encodeCallForDispatch(42, map[string]Input{"x": test.input}); err == nil || transaction != nil {
				t.Fatalf("encode = transaction %#v, error %v; want rolled-back failure", transaction, err)
			}
			cloned, released := lifecycle.snapshot()
			if len(cloned) != 0 || len(released) != 0 {
				t.Fatalf("portable media cloned/released handles: %v / %v", cloned, released)
			}
		})
	}
}

func TestTransactionalEncodingRollsBackEarlierArgumentsAndCloneFailures(t *testing.T) {
	lifecycle := installInputHandleLifecycle(t)
	if _, transaction, err := encodeCallForDispatch(42, map[string]Input{
		"a_media": fakeImageInput(),
		"z_bad":   InvalidInput("bad sibling"),
	}); err == nil || transaction != nil {
		t.Fatalf("encode = transaction %#v, error %v; want failure", transaction, err)
	}
	cloned, released := lifecycle.snapshot()
	if len(cloned) != 0 || len(released) != 0 {
		t.Fatalf("portable media cloned/released handles: %v / %v", cloned, released)
	}

	lifecycle = installInputHandleLifecycle(t)
	lifecycle.failAt = 2
	if _, transaction, err := encodeCallForDispatch(42, map[string]Input{
		"a": fakeRustTypeInput(),
		"b": fakeRustTypeInput(),
	}); err == nil || transaction != nil || !strings.Contains(err.Error(), "injected clone failure") {
		t.Fatalf("encode = transaction %#v, error %v; want clone failure", transaction, err)
	}
	cloned, released = lifecycle.snapshot()
	if !reflect.DeepEqual(cloned, []uint64{77, 77}) || !reflect.DeepEqual(released, []uint64{1001}) {
		t.Fatalf("clone/release = %v / %v, want [77 77] / [1001]", cloned, released)
	}
}

func TestRepeatedMediaInputUsesPortablePayloadWithoutHandleClones(t *testing.T) {
	lifecycle := installInputHandleLifecycle(t)
	input := fakeImageInput()
	payload, transaction, err := encodeCallForDispatch(42, map[string]Input{
		"a": input,
		"b": input,
	})
	if err != nil {
		t.Fatal(err)
	}
	defer transaction.rollback()

	var call cffi.CallFunctionArgs
	if err := proto.Unmarshal(payload, &call); err != nil {
		t.Fatal(err)
	}
	for _, argument := range call.Kwargs {
		media := argument.Value.GetMediaValue()
		if media == nil || media.GetUrl() != "https://example.test/image.png" || media.GetMimeType() != "image/png" {
			t.Fatalf("wire media = %#v, want portable URL payload", media)
		}
		if argument.Value.GetHandle() != nil {
			t.Fatalf("portable media encoded as handle: %#v", argument.Value.GetHandle())
		}
	}
	transaction.rollback()
	cloned, released := lifecycle.snapshot()
	if len(cloned) != 0 || len(released) != 0 {
		t.Fatalf("clone/release = %v / %v", cloned, released)
	}
}

func TestDeferredClassCapturesFieldInputsBeforeCallerMutatesMap(t *testing.T) {
	lifecycle := installInputHandleLifecycle(t)
	fields := map[string]Input{"media": fakeImageInput()}
	input := Class("user.MediaBox", fields)
	fields["media"] = InvalidInput("mutated after Class returned")
	delete(fields, "media")

	_, transaction, err := encodeCallForDispatch(42, map[string]Input{"x": input})
	if err != nil {
		t.Fatalf("deferred Class observed caller mutation: %v", err)
	}
	transaction.rollback()
	cloned, released := lifecycle.snapshot()
	if len(cloned) != 0 || len(released) != 0 {
		t.Fatalf("captured portable media cloned/released handles: %v / %v", cloned, released)
	}
}

func TestPostNativeCleanupCoversMalformedUnionAndLaterSiblingHandles(t *testing.T) {
	lifecycle := installInputHandleLifecycle(t)
	malformed := SelectedUnionInput(
		fakeRustTypeInput(),
		UnionBAMLType(PrimitiveBAMLType(StringType), PrimitiveBAMLType(IntType)),
		RustTypeBAMLType(),
	)
	_, transaction, err := encodeCallForDispatch(42, map[string]Input{
		"a_malformed_union": malformed,
		"z_later_handle":    fakeRustTypeInput(),
	})
	if err != nil {
		t.Fatalf("host encoding should preserve malformed metadata for native validation: %v", err)
	}
	// nativeCall synchronously validates union metadata before decoding its
	// payload and stops before the later kwarg. Call's unconditional cleanup
	// therefore must attempt both keys: neither was drained in this shape.
	transaction.rollback()
	cloned, released := lifecycle.snapshot()
	if !reflect.DeepEqual(cloned, []uint64{77, 77}) || !reflect.DeepEqual(released, []uint64{1001, 1002}) {
		t.Fatalf("clone/release = %v / %v, want every undrained key cleaned", cloned, released)
	}
}
