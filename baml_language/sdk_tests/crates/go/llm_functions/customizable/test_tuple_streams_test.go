package generated_test

import (
	"context"
	"reflect"
	"testing"

	baml_sdk "baml.local/sdk/baml_sdk"
	baml_go "github.com/boundaryml/baml-go"
)


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
	final, err := stream.Final(ctx)
	if err != nil || final != want {
		t.Fatalf("final = %#v, %v", final, err)
	}
	n, s, err := baml_sdk.TupleStreamsStreamTupleFinal(ctx, "x")
	if err != nil || n != 1 || s != "a" {
		t.Fatalf("expanded return = %v, %q, %v", n, s, err)
	}
}
