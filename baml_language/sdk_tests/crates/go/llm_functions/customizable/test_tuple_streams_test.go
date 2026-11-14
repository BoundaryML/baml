package generated_test

import (
	"context"
	"reflect"
	"testing"

	baml_sdk "baml.local/sdk/baml_sdk"
	baml_go "github.com/boundaryml/baml-go"
)

// Tuples as stream items, over `ns_tuple_streams`: each function's client is
// a scripted `StreamingClient` replaying fixed deltas, so no replay server or
// key is needed. A partial is the declared type parsed from the text so far,
// so every partial of a tuple-typed stream is a tuple: here the generated
// named struct for its tuple identity, decoded from the wire's tuple value
// (a BAML list never decodes as one).

func drainTupleStream[T any](t *testing.T, ctx context.Context, stream baml_go.Stream[T]) []T {
	t.Helper()
	var partials []T
	for {
		partial, done, err := stream.Next(ctx)
		if err != nil {
			t.Fatalf("next: %v", err)
		}
		if done {
			return partials
		}
		partials = append(partials, partial)
		if len(partials) >= 1000 {
			t.Fatal("stream failed to terminate")
		}
	}
}

// SDK_PARITY_LINT(skip): C# streams tuples from its own tuples fixture
func Test_tuples_stream_items(t *testing.T) {
	ctx := context.Background()
	stream, err := baml_sdk.TupleStreamsStreamTupleItemsStream(ctx, "x")
	if err != nil {
		t.Fatal(err)
	}
	partials := drainTupleStream(t, ctx, stream)
	if len(partials) == 0 {
		t.Fatal("expected at least one partial")
	}
	if first := partials[0]; !reflect.DeepEqual(first, []baml_sdk.Tuple2_StringInt{{Item0: "a", Item1: 1}}) {
		t.Fatalf("first partial = %#v", first)
	}
	want := []baml_sdk.Tuple2_StringInt{{Item0: "a", Item1: 1}, {Item0: "b", Item1: 2}}
	final, err := stream.Final(ctx)
	if err != nil || !reflect.DeepEqual(final, want) {
		t.Fatalf("final = %#v, %v", final, err)
	}
	// The non-streaming call settles to the same tuples.
	settled, err := baml_sdk.TupleStreamsStreamTupleItems(ctx, "x")
	if err != nil || !reflect.DeepEqual(settled, want) {
		t.Fatalf("settled = %#v, %v", settled, err)
	}
}

// SDK_PARITY_LINT(skip): C# streams tuples from its own tuples fixture
func Test_tuples_stream_final(t *testing.T) {
	ctx := context.Background()
	stream, err := baml_sdk.TupleStreamsStreamTupleFinalStream(ctx, "x")
	if err != nil {
		t.Fatal(err)
	}
	partials := drainTupleStream(t, ctx, stream)
	want := baml_sdk.Tuple2_IntString{Item0: 1, Item1: "a"}
	if len(partials) == 0 || partials[len(partials)-1] != want {
		t.Fatalf("partials = %#v", partials)
	}
	// A stream item is a value within the stream, so it stays one named
	// struct; only a top-level tuple return expands to one result per slot.
	final, err := stream.Final(ctx)
	if err != nil || final != want {
		t.Fatalf("final = %#v, %v", final, err)
	}
	n, s, err := baml_sdk.TupleStreamsStreamTupleFinal(ctx, "x")
	if err != nil || n != 1 || s != "a" {
		t.Fatalf("expanded return = %v, %q, %v", n, s, err)
	}
}
