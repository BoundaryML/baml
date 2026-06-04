package pkg

import (
	"testing"

	"bridge_go/cffi"
	pb "bridge_go/cffi/proto/baml_core/cffi/v1"

	"google.golang.org/protobuf/proto"
)

func TestBamlHandleEncodeClonesKey(t *testing.T) {
	lib, err := cffi.FindLibrary()
	if err != nil {
		t.Fatal(err)
	}
	if err := cffi.Init(lib); err != nil {
		t.Fatal(err)
	}

	key, handleType, err := cffi.HandleTestSeedFunctionRef(123)
	if err != nil {
		t.Fatal(err)
	}
	original := BamlHandle{Key: key, HandleType: handleType}
	defer func() {
		_ = original.Release()
	}()

	encoded, err := encodeCallArgs(map[string]any{"h": original})
	if err != nil {
		t.Fatal(err)
	}

	var args pb.CallFunctionArgs
	if err := proto.Unmarshal(encoded, &args); err != nil {
		t.Fatal(err)
	}
	if len(args.Kwargs) != 1 {
		t.Fatalf("expected one kwarg, got %d", len(args.Kwargs))
	}
	wire := args.Kwargs[0].GetValue().GetHandle()
	if wire == nil {
		t.Fatal("expected encoded handle")
	}
	if wire.Key == original.Key {
		t.Fatalf("expected encode to clone key %d, got original key", original.Key)
	}
	if int32(wire.HandleType) != original.HandleType {
		t.Fatalf("expected wire handle type %d, got %d", original.HandleType, wire.HandleType)
	}
	if err := original.Validate(); err != nil {
		t.Fatalf("original handle should remain valid after encode: %v", err)
	}
	if err := (BamlHandle{Key: wire.Key, HandleType: int32(wire.HandleType)}).Release(); err != nil {
		t.Fatalf("wire clone should be independently releasable: %v", err)
	}
}
