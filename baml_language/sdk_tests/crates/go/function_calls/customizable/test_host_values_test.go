package sdk_test

import (
	"context"
	"errors"
	"testing"

	baml_sdk "baml.local/sdk/baml_sdk"
	"baml.local/sdk/baml_sdk/baml"
	baml_go "github.com/boundaryml/baml-go"
)

// A host's own value (`baml.host.Value`) is registered by the bridge, held by
// BAML by key, and comes back as the same Go value.
func Test_host_values_round_trip_as_the_same_value(t *testing.T) {
	ctx := context.Background()
	original := &struct{ name string }{"mine"}
	sent := baml_go.NewHostValue(original)
	back, err := baml_sdk.HostValueTestsEchoHostValue(ctx, sent)
	if err != nil {
		t.Fatal(err)
	}
	if back.Value() != any(original) {
		t.Fatalf("got %v back, want the value sent", back.Value())
	}
	if !back.Equal(sent) {
		t.Fatal("the value back is not Equal to the value sent")
	}
	// Negative control: another host value around the same Go value was
	// never sent.
	if back.Equal(baml_go.NewHostValue(original)) {
		t.Fatal("an unsent host value is Equal to a held one")
	}
}

func Test_host_values_come_back_inside_a_list(t *testing.T) {
	ctx := context.Background()
	sent := baml_go.NewHostValue(7)
	back, err := baml_sdk.HostValueTestsTwiceHostValue(ctx, sent)
	if err != nil {
		t.Fatal(err)
	}
	if len(back) != 2 {
		t.Fatalf("got %d values, want 2", len(back))
	}
	for _, value := range back {
		if value.Value() != any(7) || !value.Equal(sent) {
			t.Fatalf("got %v back, want the value sent", value.Value())
		}
	}
}

func Test_host_values_sent_twice_are_one_value_to_baml(t *testing.T) {
	ctx := context.Background()
	sent := baml_go.NewHostValue(3)
	same, err := baml_sdk.HostValueTestsSameHostValue(ctx, sent, sent)
	if err != nil {
		t.Fatal(err)
	}
	if !same {
		t.Fatal("one host value sent twice is two values to BAML")
	}
	// Negative control: two host values are two values.
	other, err := baml_sdk.HostValueTestsSameHostValue(ctx, sent, baml_go.NewHostValue(3))
	if err != nil {
		t.Fatal(err)
	}
	if other {
		t.Fatal("two host values are one value to BAML")
	}
}

// A host value BAML sends back stays readable until the result that carried
// it is done, even when BAML drops its own last reference and collects
// garbage before Go reads it.
func Test_host_value_back_decodes_after_baml_collects_garbage(t *testing.T) {
	ctx := context.Background()
	if err := baml.SysCollectGarbage(ctx); err != nil {
		t.Fatal(err)
	}
	for range 16 {
		sent := baml_go.NewHostValue(&struct{ name string }{"collected"})
		result, err := baml_go.Call(ctx, "user.host_value_tests.echo_host_value", map[string]baml_go.Input{
			"value": baml_go.HostValueInput(sent),
		})
		if err != nil {
			t.Fatal(err)
		}
		if err := baml.SysCollectGarbage(ctx); err != nil {
			t.Fatal(err)
		}
		back, err := result.HostValue()
		if err != nil {
			t.Fatalf("decode after a collection: %v", err)
		}
		if !back.Equal(sent) || back.Value() != sent.Value() {
			t.Fatalf("got %v back, want the value sent", back.Value())
		}
		// Sent again after BAML let it go, it is still one value to BAML.
		same, err := baml_sdk.HostValueTestsSameHostValue(ctx, back, sent)
		if err != nil || !same {
			t.Fatalf("the value back is another value to BAML: %v, %v", same, err)
		}
	}
}

// Two decodes of one host value BAML sent back are one host value.
func Test_host_value_decoded_twice_is_one_value(t *testing.T) {
	ctx := context.Background()
	sent := baml_go.NewHostValue("twice")
	back, err := baml_sdk.HostValueTestsTwiceHostValue(ctx, sent)
	if err != nil {
		t.Fatal(err)
	}
	if len(back) != 2 || !back[0].Equal(back[1]) {
		t.Fatalf("got %v, want one host value twice", back)
	}
}

// The Go error a callback raised crosses as a host key inside the
// baml.errors.HostCallable BAML builds around it. Go holds that error after
// BAML has let it go, and can still throw it on after a collection.
func Test_host_error_in_an_error_envelope_stays_usable(t *testing.T) {
	ctx := context.Background()
	_, first := baml_sdk.HostCallableTestsPropagateThrowingCallback(ctx, func(context.Context, int64) (string, error) {
		return "", errors.New("raised by Go")
	}, 1)
	var failure *baml_go.BamlError
	if !errors.As(first, &failure) || failure.ClassName != "baml.errors.HostCallable" {
		t.Fatalf("first failure = %#v, want a baml.errors.HostCallable", first)
	}
	if err := baml.SysCollectGarbage(ctx); err != nil {
		t.Fatal(err)
	}
	_, second := baml_sdk.HostCallableTestsPropagateThrowingCallback(ctx, func(context.Context, int64) (string, error) {
		return "", first
	}, 2)
	var forwarded *baml_go.BamlError
	if !errors.As(second, &forwarded) || forwarded.Kind != "error" || forwarded.ClassName != "baml.errors.HostCallable" || forwarded.Message != failure.Message {
		t.Fatalf("forwarded failure = %#v, want the first thrown on", second)
	}
}
