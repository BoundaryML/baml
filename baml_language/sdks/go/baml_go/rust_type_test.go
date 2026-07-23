package baml_go

import (
	"errors"
	"runtime"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/boundaryml/baml-go/internal/cffi"
)

func rustTypeWireHandle(key uint64) *cffi.BamlOutboundHandle {
	return &cffi.BamlOutboundHandle{Key: key, HandleType: cffi.BamlHandleType_UNTAGGED_RUST_DATA}
}

func rustTypeWireValue(handle *cffi.BamlOutboundHandle) Value {
	return Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_HandleValue{HandleValue: handle}}}
}

func TestRustTypeDecodeOwnsCloneAndFinalizesExactlyOnce(t *testing.T) {
	originalClone := cloneOutboundRustTypeHandle
	originalRelease := releaseRustTypeHandle
	t.Cleanup(func() {
		cloneOutboundRustTypeHandle = originalClone
		releaseRustTypeHandle = originalRelease
	})

	cloneOutboundRustTypeHandle = func(key uint64) (uint64, error) {
		if key != 41 {
			t.Fatalf("cloned key %d, want 41", key)
		}
		return 42, nil
	}
	var releases atomic.Int32
	releaseRustTypeHandle = func(key uint64) {
		if key != 42 {
			t.Errorf("released key %d, want 42", key)
		}
		releases.Add(1)
	}

	decoded, err := rustTypeWireValue(rustTypeWireHandle(41)).RustType()
	if err != nil {
		t.Fatal(err)
	}
	copyOfDecoded := decoded
	if decoded.handle == nil || copyOfDecoded.handle != decoded.handle {
		t.Fatal("Go copies must share one owned handle")
	}
	finalizeRustTypeHandle(decoded.handle)
	finalizeRustTypeHandle(copyOfDecoded.handle)
	if got := releases.Load(); got != 1 {
		t.Fatalf("released %d times, want exactly once", got)
	}
	if input := RustTypeInput(decoded); input.err == nil {
		t.Fatal("released RustType must be rejected on re-entry")
	}
}

func TestRustTypeFinalizerEventuallyReleasesOwnedClone(t *testing.T) {
	originalClone := cloneOutboundRustTypeHandle
	originalRelease := releaseRustTypeHandle
	t.Cleanup(func() {
		cloneOutboundRustTypeHandle = originalClone
		releaseRustTypeHandle = originalRelease
	})

	cloneOutboundRustTypeHandle = func(uint64) (uint64, error) { return 52, nil }
	released := make(chan uint64, 1)
	releaseRustTypeHandle = func(key uint64) { released <- key }
	func() {
		value, err := rustTypeWireValue(rustTypeWireHandle(51)).RustType()
		if err != nil {
			t.Fatal(err)
		}
		if value.handle == nil {
			t.Fatal("decoded RustType has no handle")
		}
	}()

	deadline := time.After(5 * time.Second)
	for {
		runtime.GC()
		select {
		case key := <-released:
			if key != 52 {
				t.Fatalf("released key %d, want 52", key)
			}
			return
		case <-deadline:
			t.Fatal("RustType finalizer did not release its handle")
		case <-time.After(10 * time.Millisecond):
		}
	}
}

func TestRustTypeDecodeRejectsMalformedAndForeignHandles(t *testing.T) {
	originalClone := cloneOutboundRustTypeHandle
	t.Cleanup(func() { cloneOutboundRustTypeHandle = originalClone })
	cloneOutboundRustTypeHandle = func(uint64) (uint64, error) {
		t.Fatal("malformed handles must be rejected before cloning")
		return 0, nil
	}

	cases := []struct {
		name   string
		handle *cffi.BamlOutboundHandle
	}{
		{"missing", nil},
		{"zero", rustTypeWireHandle(0)},
		{"wrong kind", &cffi.BamlOutboundHandle{Key: 1, HandleType: cffi.BamlHandleType_ADT_COLLECTOR}},
		{"tagged metadata", &cffi.BamlOutboundHandle{Key: 1, HandleType: cffi.BamlHandleType_UNTAGGED_RUST_DATA, Ty: RustTypeBAMLType().value}},
	}
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			if _, err := rustTypeWireValue(test.handle).RustType(); err == nil {
				t.Fatal("expected malformed handle rejection")
			}
		})
	}

	if _, err := (Value{}).RustType(); err == nil {
		t.Fatal("uninitialized Value must be rejected")
	}
	if _, err := (Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "not a handle"}}}).RustType(); err == nil {
		t.Fatal("non-handle value must be rejected")
	}
}

func TestRustTypeDecodeRejectsCloneFailureAndZeroClone(t *testing.T) {
	originalClone := cloneOutboundRustTypeHandle
	t.Cleanup(func() { cloneOutboundRustTypeHandle = originalClone })

	cloneOutboundRustTypeHandle = func(uint64) (uint64, error) { return 0, errors.New("stale") }
	if _, err := rustTypeWireValue(rustTypeWireHandle(61)).RustType(); err == nil {
		t.Fatal("stale native handle must be rejected")
	}
	cloneOutboundRustTypeHandle = func(uint64) (uint64, error) { return 0, nil }
	if _, err := rustTypeWireValue(rustTypeWireHandle(61)).RustType(); err == nil {
		t.Fatal("zero native clone must be rejected")
	}
}

func TestRustTypeInputClonesTransactionallyAndRollsBack(t *testing.T) {
	originalClone := cloneInboundHandle
	originalRelease := releaseInboundHandle
	t.Cleanup(func() {
		cloneInboundHandle = originalClone
		releaseInboundHandle = originalRelease
	})

	cloneInboundHandle = func(key uint64) (uint64, error) {
		if key != 71 {
			t.Fatalf("cloned key %d, want 71", key)
		}
		return 72, nil
	}
	var released []uint64
	releaseInboundHandle = func(key uint64) { released = append(released, key) }
	input := RustTypeInput(RustType{handle: &rustTypeHandle{key: 71}})
	transaction := &inputTransaction{}
	encoded, err := input.encodeValue(transaction)
	if err != nil {
		t.Fatal(err)
	}
	handle := encoded.GetHandle()
	if handle == nil || handle.Key != 72 || handle.HandleType != cffi.BamlHandleType_UNTAGGED_RUST_DATA {
		t.Fatalf("unexpected inbound handle: %#v", handle)
	}
	if len(transaction.keys) != 1 || transaction.keys[0] != 72 {
		t.Fatalf("transaction owns %#v, want [72]", transaction.keys)
	}
	transaction.rollback()
	transaction.rollback()
	if len(released) != 1 || released[0] != 72 {
		t.Fatalf("rollback releases %#v, want [72] exactly once", released)
	}
}

func TestRustTypeInputIsSafeForConcurrentRepeatedUse(t *testing.T) {
	originalClone := cloneInboundHandle
	originalRelease := releaseInboundHandle
	t.Cleanup(func() {
		cloneInboundHandle = originalClone
		releaseInboundHandle = originalRelease
	})

	var next atomic.Uint64
	next.Store(100)
	cloneInboundHandle = func(key uint64) (uint64, error) {
		if key != 81 {
			t.Errorf("cloned key %d, want 81", key)
		}
		return next.Add(1), nil
	}
	var released atomic.Int32
	releaseInboundHandle = func(uint64) { released.Add(1) }
	value := RustType{handle: &rustTypeHandle{key: 81}}

	const uses = 64
	var group sync.WaitGroup
	for index := 0; index < uses; index++ {
		group.Add(1)
		go func() {
			defer group.Done()
			transaction := &inputTransaction{}
			encoded, err := RustTypeInput(value).encodeValue(transaction)
			if err != nil {
				t.Error(err)
				return
			}
			if encoded.GetHandle() == nil {
				t.Error("concurrent encode returned no handle")
			}
			transaction.rollback()
		}()
	}
	group.Wait()
	if got := released.Load(); got != uses {
		t.Fatalf("released %d transaction clones, want %d", got, uses)
	}
}

func TestRustTypeDescriptorAndDynamicInputAreExact(t *testing.T) {
	if !RustTypeBAMLType().Equal(BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_RustType{RustType: &cffi.BamlTyRustType{}}}}) {
		t.Fatal("RustType descriptor is not the canonical wire leaf")
	}
	if RustTypeBAMLType().Equal(MetaTypeBAMLType()) {
		t.Fatal("RustType descriptor must not equal the type metatype")
	}
	zero := RustType{}
	if input := Any(zero); input.err == nil {
		t.Fatal("dynamic zero RustType must be rejected")
	}
}
