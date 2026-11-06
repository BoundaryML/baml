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

func handleWireValue(handle *cffi.BamlOutboundHandle) Value {
	return Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_HandleValue{HandleValue: handle}}}
}

func sessionWireHandle(key uint64) *cffi.BamlOutboundHandle {
	return &cffi.BamlOutboundHandle{
		Key:        key,
		HandleType: cffi.BamlHandleType_ADT_RUNTIME_VALUE,
		Ty:         &cffi.BamlTy{Ty: &cffi.BamlTy_ClassTy{ClassTy: &cffi.BamlTyClass{Head: &cffi.BamlTyClass_Name{Name: "reflect.Session"}}}},
	}
}

// A decode owns its own clone of the handle; Go copies share it, and it is
// released exactly once.
func TestOpaqueHandleDecodeOwnsCloneAndFinalizesExactlyOnce(t *testing.T) {
	originalClone := cloneOutboundHandle
	originalRelease := releaseOutboundHandle
	t.Cleanup(func() {
		cloneOutboundHandle = originalClone
		releaseOutboundHandle = originalRelease
	})
	cloneOutboundHandle = func(key uint64) (uint64, error) {
		if key != 41 {
			t.Fatalf("cloned key %d, want 41", key)
		}
		return 42, nil
	}
	var releases atomic.Int32
	releaseOutboundHandle = func(key uint64) {
		if key != 42 {
			t.Errorf("released key %d, want 42", key)
		}
		releases.Add(1)
	}

	decoded, err := handleWireValue(sessionWireHandle(41)).OpaqueHandle("reflect.Session")
	if err != nil {
		t.Fatal(err)
	}
	copyOfDecoded := decoded
	if decoded.handle == nil || copyOfDecoded.handle != decoded.handle {
		t.Fatal("Go copies must share one owned handle")
	}
	finalizeOpaqueHandle(decoded.handle)
	finalizeOpaqueHandle(copyOfDecoded.handle)
	if got := releases.Load(); got != 1 {
		t.Fatalf("released %d times, want exactly once", got)
	}
	if input := OpaqueHandleInput(decoded); input.err == nil {
		t.Fatal("a released OpaqueHandle must be rejected on re-entry")
	}
}

func TestOpaqueHandleFinalizerEventuallyReleasesOwnedClone(t *testing.T) {
	originalClone := cloneOutboundHandle
	originalRelease := releaseOutboundHandle
	t.Cleanup(func() {
		cloneOutboundHandle = originalClone
		releaseOutboundHandle = originalRelease
	})
	cloneOutboundHandle = func(uint64) (uint64, error) { return 52, nil }
	released := make(chan uint64, 1)
	releaseOutboundHandle = func(key uint64) { released <- key }
	func() {
		value, err := handleWireValue(sessionWireHandle(51)).OpaqueHandle("reflect.Session")
		if err != nil {
			t.Fatal(err)
		}
		if value.handle == nil {
			t.Fatal("decoded OpaqueHandle has no handle")
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
			t.Fatal("OpaqueHandle finalizer did not release its handle")
		case <-time.After(10 * time.Millisecond):
		}
	}
}

func TestOpaqueHandleDecodeRejectsMalformedAndForeignHandles(t *testing.T) {
	originalClone := cloneOutboundHandle
	t.Cleanup(func() { cloneOutboundHandle = originalClone })
	cloneOutboundHandle = func(uint64) (uint64, error) {
		t.Fatal("malformed handles must be rejected before cloning")
		return 0, nil
	}

	cases := []struct {
		name   string
		handle *cffi.BamlOutboundHandle
	}{
		{"missing", nil},
		{"zero", sessionWireHandle(0)},
		{"wrong kind", &cffi.BamlOutboundHandle{Key: 1, HandleType: cffi.BamlHandleType_ADT_MEDIA_IMAGE, Ty: sessionWireHandle(1).Ty}},
		{"other class", &cffi.BamlOutboundHandle{
			Key:        1,
			HandleType: cffi.BamlHandleType_ADT_RUNTIME_VALUE,
			Ty:         &cffi.BamlTy{Ty: &cffi.BamlTy_ClassTy{ClassTy: &cffi.BamlTyClass{Head: &cffi.BamlTyClass_Name{Name: "reflect.Package"}}}},
		}},
	}
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			if _, err := handleWireValue(test.handle).OpaqueHandle("reflect.Session"); err == nil {
				t.Fatal("expected malformed handle rejection")
			}
		})
	}

	if _, err := (Value{}).OpaqueHandle("reflect.Session"); err == nil {
		t.Fatal("uninitialized Value must be rejected")
	}
	if _, err := (Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "not a handle"}}}).OpaqueHandle("reflect.Session"); err == nil {
		t.Fatal("non-handle value must be rejected")
	}
}

func TestOpaqueHandleDecodeRejectsCloneFailureAndZeroClone(t *testing.T) {
	originalClone := cloneOutboundHandle
	t.Cleanup(func() { cloneOutboundHandle = originalClone })

	cloneOutboundHandle = func(uint64) (uint64, error) { return 0, errors.New("stale") }
	if _, err := handleWireValue(sessionWireHandle(61)).OpaqueHandle("reflect.Session"); err == nil {
		t.Fatal("stale native handle must be rejected")
	}
	cloneOutboundHandle = func(uint64) (uint64, error) { return 0, nil }
	if _, err := handleWireValue(sessionWireHandle(61)).OpaqueHandle("reflect.Session"); err == nil {
		t.Fatal("zero native clone must be rejected")
	}
}

func TestOpaqueHandleInputClonesTransactionallyAndRollsBack(t *testing.T) {
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

	transaction := &inputTransaction{}
	encoded, err := OpaqueHandleInput(OpaqueHandle{handle: &opaqueHandle{key: 71}}).encodeValue(transaction)
	if err != nil {
		t.Fatal(err)
	}
	handle := encoded.GetHandle()
	if handle == nil || handle.Key != 72 || handle.HandleType != cffi.BamlHandleType_ADT_RUNTIME_VALUE {
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

func TestOpaqueHandleInputIsSafeForConcurrentRepeatedUse(t *testing.T) {
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
	value := OpaqueHandle{handle: &opaqueHandle{key: 81}}

	const uses = 64
	var group sync.WaitGroup
	for range uses {
		group.Add(1)
		go func() {
			defer group.Done()
			transaction := &inputTransaction{}
			encoded, err := OpaqueHandleInput(value).encodeValue(transaction)
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

// Every engine-built instance decodes as a dynamic value, whatever names its
// class: an instance of a run-time declaration is labelled, not named.
func TestRuntimeValueOfAnyDeclarationDecodesDynamically(t *testing.T) {
	originalClone := cloneOutboundHandle
	originalRelease := releaseOutboundHandle
	t.Cleanup(func() {
		cloneOutboundHandle = originalClone
		releaseOutboundHandle = originalRelease
	})
	cloneOutboundHandle = func(key uint64) (uint64, error) { return key + 100, nil }
	releaseOutboundHandle = func(uint64) {}
	previousInboundClone := cloneInboundHandle
	cloneInboundHandle = func(key uint64) (uint64, error) { return key + 1, nil }
	t.Cleanup(func() { cloneInboundHandle = previousInboundClone })

	for _, ty := range []*cffi.BamlTy{
		{Ty: &cffi.BamlTy_ClassTy{ClassTy: &cffi.BamlTyClass{Head: &cffi.BamlTyClass_AnonymousLabel{AnonymousLabel: "RuntimeRecord"}}}},
		{Ty: &cffi.BamlTy_Enum{Enum: &cffi.BamlTyEnum{Head: &cffi.BamlTyEnum_AnonymousLabel{AnonymousLabel: "Category"}}}},
		{Ty: &cffi.BamlTy_ClassTy{ClassTy: &cffi.BamlTyClass{Head: &cffi.BamlTyClass_Name{Name: "reflect.Session"}}}},
	} {
		value := handleWireValue(&cffi.BamlOutboundHandle{Key: 7, HandleType: cffi.BamlHandleType_ADT_RUNTIME_VALUE, Ty: ty})
		decoded, err := DecodeAs[any](value)
		if err != nil {
			t.Fatalf("decode %v: %v", ty, err)
		}
		handle, ok := decoded.(OpaqueHandle)
		if !ok || handle.handle == nil || handle.handle.key != 107 {
			t.Fatalf("decode %v = %#v, want an OpaqueHandle owning its own clone", ty, decoded)
		}
		encoded, err := handle.BAMLInput().encodeValue(&inputTransaction{})
		if err != nil || encoded.GetHandle().GetHandleType() != cffi.BamlHandleType_ADT_RUNTIME_VALUE {
			t.Fatalf("re-send %v = %v, %v; want the handle it crossed as", ty, encoded, err)
		}
	}
}

// The runtime keys one instance by one handle while any reference to it is
// held, so two decodes of one instance are Equal and another instance's are
// not.
func TestOpaqueHandleEqualityIsInstanceIdentity(t *testing.T) {
	originalClone := cloneOutboundHandle
	originalRelease := releaseOutboundHandle
	t.Cleanup(func() {
		cloneOutboundHandle = originalClone
		releaseOutboundHandle = originalRelease
	})
	cloneOutboundHandle = func(key uint64) (uint64, error) { return key, nil }
	releaseOutboundHandle = func(uint64) {}

	decode := func(key uint64) OpaqueHandle {
		t.Helper()
		value := handleWireValue(&cffi.BamlOutboundHandle{
			Key:        key,
			HandleType: cffi.BamlHandleType_ADT_RUNTIME_VALUE,
			Ty:         &cffi.BamlTy{Ty: &cffi.BamlTy_ClassTy{ClassTy: &cffi.BamlTyClass{Head: &cffi.BamlTyClass_Name{Name: "reflect.Session"}}}},
		})
		handle, err := value.OpaqueHandle("reflect.Session")
		if err != nil {
			t.Fatalf("decode %d: %v", key, err)
		}
		return handle
	}
	first, second, other := decode(7), decode(7), decode(8)
	if !first.Equal(second) {
		t.Fatal("two decodes of one instance are not Equal")
	}
	if first.Equal(other) {
		t.Fatal("decodes of two instances are Equal")
	}
	if first.Equal(OpaqueHandle{}) || !(OpaqueHandle{}).Equal(OpaqueHandle{}) {
		t.Fatal("only the zero OpaqueHandle equals the zero OpaqueHandle")
	}
}
