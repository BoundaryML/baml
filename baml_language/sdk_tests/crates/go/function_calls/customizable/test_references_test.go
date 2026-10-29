package sdk_test

// Instances the runtime keeps to itself (`baml.hash.DefaultHasher`) cross as
// references: one passed back is the same instance, wherever a class can sit.
// Its state shows which instance a call reached: one hasher fed "a" and then
// "b" hashes as a fresh one fed "ab".

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
	"baml.local/sdk/baml_sdk/baml"
)

func makeHasher(t *testing.T, ctx context.Context) baml.HashDefaultHasher {
	t.Helper()
	hasher, err := baml_sdk.ReferencesMakeHasher(ctx)
	if err != nil {
		t.Fatalf("make hasher: %v", err)
	}
	return hasher
}

func hashAfter(t *testing.T, ctx context.Context, hasher baml.HashDefaultHasher, text string) int64 {
	t.Helper()
	hash, err := baml_sdk.ReferencesHashAfter(ctx, hasher, text)
	if err != nil {
		t.Fatalf("hash after %q: %v", text, err)
	}
	return hash
}

func Test_references_a_returned_instance_passed_back_is_the_same_instance(t *testing.T) {
	ctx := context.Background()
	hasher := makeHasher(t, ctx)
	hashAfter(t, ctx, hasher, "a")
	fedTwice := hashAfter(t, ctx, hasher, "b")
	if want := hashAfter(t, ctx, makeHasher(t, ctx), "ab"); fedTwice != want {
		t.Fatalf("hasher fed a then b = %d, fresh hasher fed ab = %d", fedTwice, want)
	}
	// Negative control: a fresh hasher fed only "b" differs.
	if fedTwice == hashAfter(t, ctx, makeHasher(t, ctx), "b") {
		t.Fatal("the hasher's earlier write was lost")
	}
}

func Test_references_one_engine_instance_is_one_reference(t *testing.T) {
	ctx := context.Background()
	hasher := makeHasher(t, ctx)
	same, err := baml_sdk.ReferencesSameHasher(ctx, hasher)
	if err != nil || !same.Equal(hasher) {
		t.Fatalf("same hasher = %#v, %v; want the same instance", same, err)
	}
	if makeHasher(t, ctx).Equal(hasher) {
		t.Fatal("two engine instances compared equal")
	}
}

func Test_references_containers_hold_the_instance(t *testing.T) {
	ctx := context.Background()
	hasher := makeHasher(t, ctx)
	if present, err := baml_sdk.ReferencesMaybeHasher(ctx, true); err != nil || present == nil {
		t.Fatalf("maybe hasher(true) = %v, %v", present, err)
	}
	if absent, err := baml_sdk.ReferencesMaybeHasher(ctx, false); err != nil || absent != nil {
		t.Fatalf("maybe hasher(false) = %v, %v", absent, err)
	}
	twice, err := baml_sdk.ReferencesHasherTwice(ctx, hasher)
	if err != nil || len(twice) != 2 || !twice[0].Equal(hasher) || !twice[1].Equal(hasher) {
		t.Fatalf("hasher twice = %#v, %v", twice, err)
	}
	under, err := baml_sdk.ReferencesHasherUnder(ctx, "key", hasher)
	if err != nil || len(under) != 1 || !under["key"].Equal(hasher) {
		t.Fatalf("hasher under = %#v, %v", under, err)
	}
}

func Test_references_a_field_holds_the_instance(t *testing.T) {
	ctx := context.Background()
	hasher := makeHasher(t, ctx)
	holder, err := baml_sdk.ReferencesHoldHasher(ctx, "label", hasher)
	if err != nil || holder.Label != "label" || !holder.Hasher.Equal(hasher) {
		t.Fatalf("holder = %#v, %v", holder, err)
	}
}

func Test_references_a_union_selects_the_instance_arm(t *testing.T) {
	ctx := context.Background()
	hasherArm, err := baml_sdk.ReferencesHasherOrMarker(ctx, true)
	if err != nil || hasherArm.Kind() != baml_sdk.HashDefaultHasherOrReferencesHasherMarkerKindHashDefaultHasher {
		t.Fatalf("hasher or marker(true) = %#v, %v", hasherArm, err)
	}
	markerArm, err := baml_sdk.ReferencesHasherOrMarker(ctx, false)
	if err != nil || markerArm.Kind() != baml_sdk.HashDefaultHasherOrReferencesHasherMarkerKindReferencesHasherMarker {
		t.Fatalf("hasher or marker(false) = %#v, %v", markerArm, err)
	}
	described, err := baml_sdk.ReferencesDescribeHasherOrMarker(
		ctx, baml_sdk.NewHashDefaultHasherOrReferencesHasherMarkerFromHashDefaultHasher(makeHasher(t, ctx)))
	if err != nil || described != "hasher" {
		t.Fatalf("describe hasher = %q, %v", described, err)
	}
	described, err = baml_sdk.ReferencesDescribeHasherOrMarker(
		ctx, baml_sdk.NewHashDefaultHasherOrReferencesHasherMarkerFromReferencesHasherMarker(baml_sdk.ReferencesHasherMarker{Tag: "marker"}))
	if err != nil || described != "marker" {
		t.Fatalf("describe marker = %q, %v", described, err)
	}
}

func Test_references_a_callback_receives_and_returns_the_instance(t *testing.T) {
	ctx := context.Background()
	hasher := makeHasher(t, ctx)
	var received []baml.HashDefaultHasher
	callback := func(_ context.Context, value baml.HashDefaultHasher) baml.HashDefaultHasher {
		received = append(received, value)
		return value
	}
	returned, err := baml_sdk.ReferencesHasherThroughCallback(ctx, hasher, callback)
	if err != nil || !returned.Equal(hasher) {
		t.Fatalf("through callback = %#v, %v; want the same instance", returned, err)
	}
	if len(received) != 1 || !received[0].Equal(hasher) {
		t.Fatalf("callback received %#v; want the same instance once", received)
	}
}
