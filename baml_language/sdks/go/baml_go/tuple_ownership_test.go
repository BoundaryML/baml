package baml_go

import (
	"reflect"
	"runtime"
	"slices"
	"sync"
	"testing"
	"time"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// testImageString stands in for a generated named tuple struct whose first
// slot holds a handle.
type testImageString struct {
	Item0 Image
	Item1 string
}

func imageStringTuple() BAMLType {
	return TupleBAMLType(ImageBAMLType(), PrimitiveBAMLType(StringType))
}

func (testImageString) BAMLType() BAMLType { return imageStringTuple() }

func (testImageString) BAMLDecode(value Value) (any, error) {
	slots, err := value.Tuple(2)
	if err != nil {
		return nil, err
	}
	first, err := DecodeTupleSlot(slots, 0, Value.Image)
	if err != nil {
		return nil, err
	}
	second, err := DecodeTupleSlot(slots, 1, Value.String)
	if err != nil {
		return nil, err
	}
	return testImageString{Item0: first, Item1: second}, nil
}

// tupleMediaHooks counts media clones and records media releases, which a
// finalizer may report from its own goroutine.
type tupleMediaHooks struct {
	mu       sync.Mutex
	clones   []uint64
	released []uint64
}

func installTupleMediaHooks(t *testing.T) *tupleMediaHooks {
	t.Helper()
	hooks := &tupleMediaHooks{}
	previousValidate := validateOutboundMediaHandle
	previousClone := cloneOutboundMediaHandle
	previousRelease := releaseMediaHandle
	next := uint64(900)
	validateOutboundMediaHandle = func(uint64, cffi.BamlHandleType) error { return nil }
	cloneOutboundMediaHandle = func(uint64) (uint64, error) {
		hooks.mu.Lock()
		defer hooks.mu.Unlock()
		next++
		hooks.clones = append(hooks.clones, next)
		return next, nil
	}
	releaseMediaHandle = func(key uint64) {
		hooks.mu.Lock()
		defer hooks.mu.Unlock()
		hooks.released = append(hooks.released, key)
	}
	t.Cleanup(func() {
		validateOutboundMediaHandle = previousValidate
		cloneOutboundMediaHandle = previousClone
		releaseMediaHandle = previousRelease
	})
	return hooks
}

func (hooks *tupleMediaHooks) snapshot() (clones, released []uint64) {
	hooks.mu.Lock()
	defer hooks.mu.Unlock()
	return append([]uint64(nil), hooks.clones...), append([]uint64(nil), hooks.released...)
}

// awaitReleased collects garbage until every key in want is released.
func (hooks *tupleMediaHooks) awaitReleased(t *testing.T, want []uint64) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for {
		_, released := hooks.snapshot()
		slices.Sort(released)
		if reflect.DeepEqual(released, want) {
			return
		}
		if time.Now().After(deadline) {
			t.Fatalf("released media clones = %v, want %v", released, want)
		}
		runtime.GC()
		time.Sleep(10 * time.Millisecond)
	}
}

func tupleResult(t *testing.T, tupleType BAMLType, items ...*cffi.BamlOutboundValue) Value {
	t.Helper()
	tuple := carriedTuple(tupleType, items...)
	value, err := decodeResultEnvelope(&cffi.BamlOutboundResult{Result: &cffi.BamlOutboundResult_Ok{Ok: tuple.value}})
	if err != nil {
		t.Fatal(err)
	}
	return value
}

// A registered tuple whose later slot fails to decode is an error, not a
// fallback: the earlier slot's handle is decoded once, the clone it took is
// released when the partial struct is dropped, and the result's owner still
// releases the engine's original key exactly once.
func TestTupleLaterSlotFailureReleasesEarlierSlotHandles(t *testing.T) {
	RegisterTuple[testImageString]()
	hooks := installTupleMediaHooks(t)
	ownerReleased := installOutboundReleaseRecorder(t)

	value := tupleResult(t, imageStringTuple(), outboundMediaHandle(91), outboundInt(7))
	if got, err := DecodeAs[any](value); err == nil {
		t.Fatalf("a tuple with a malformed slot decoded as %#v", got)
	}
	clones, _ := hooks.snapshot()
	if len(clones) != 1 {
		t.Fatalf("earlier slot decoded %d times (clones %v), want once and no fallback re-decode", len(clones), clones)
	}
	hooks.awaitReleased(t, clones)

	runtime.SetFinalizer(value.owner, nil)
	finalizeResultOwner(value.owner)
	finalizeResultOwner(value.owner)
	if !reflect.DeepEqual(*ownerReleased, []uint64{91}) {
		t.Fatalf("owner released %v, want [91] exactly once", *ownerReleased)
	}
}

// An unregistered tuple falls back to a list of its slots, and the slots are
// decoded once: each handle is cloned once, for the one Image the list holds.
func TestTupleListFallbackDecodesEachHandleOnce(t *testing.T) {
	hooks := installTupleMediaHooks(t)
	ownerReleased := installOutboundReleaseRecorder(t)

	imagePair := TupleBAMLType(ImageBAMLType(), ImageBAMLType())
	value := tupleResult(t, imagePair, outboundMediaHandle(92), outboundMediaHandle(93))
	got, err := DecodeAs[any](value)
	if err != nil {
		t.Fatal(err)
	}
	list, ok := got.([]any)
	if !ok || len(list) != 2 {
		t.Fatalf("unregistered (image, image) = %#v", got)
	}
	clones, _ := hooks.snapshot()
	if len(clones) != 2 {
		t.Fatalf("slots cloned %v, want one clone per slot", clones)
	}
	for index, item := range list {
		image, ok := item.(Image)
		if !ok || image.media.handle.key != clones[index] {
			t.Fatalf("slot %d = %#v, want the Image holding clone %d", index, item, clones[index])
		}
	}
	list = nil
	got = nil
	hooks.awaitReleased(t, clones)

	runtime.SetFinalizer(value.owner, nil)
	finalizeResultOwner(value.owner)
	slices.Sort(*ownerReleased)
	if !reflect.DeepEqual(*ownerReleased, []uint64{92, 93}) {
		t.Fatalf("owner released %v, want [92 93] exactly once", *ownerReleased)
	}
}
