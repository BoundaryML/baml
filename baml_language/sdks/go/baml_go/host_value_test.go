package baml_go

import (
	"reflect"
	"sync"
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
)

func hostValueWire(key uint64) Value {
	return handleWireValue(&cffi.BamlOutboundHandle{Key: key, HandleType: cffi.BamlHandleType_HOST_VALUE_OPAQUE})
}

// hostKeyCount is how many references to key Go has handed out, or 0 when
// it is not registered.
func hostKeyCount(key uint64) uint64 {
	hostValues.Lock()
	defer hostValues.Unlock()
	if registration, ok := hostValues.table[key]; ok {
		return registration.count
	}
	return 0
}

func sendHostValue(t *testing.T, value HostValue) (uint64, *inputTransaction) {
	t.Helper()
	transaction := &inputTransaction{}
	encoded, err := HostValueInput(value).encodeValue(transaction)
	if err != nil {
		t.Fatal(err)
	}
	handle := encoded.GetHandle()
	if handle.GetHandleType() != cffi.BamlHandleType_HOST_VALUE_OPAQUE || handle.GetKey() == 0 {
		t.Fatalf("sent %v, want a host-value handle", handle)
	}
	return handle.GetKey(), transaction
}

// A host value is registered when first sent, sent again under the same
// registration (one more reference) while BAML holds it, and decodes back to
// the same host value.
func TestHostValueRoundTripsAsItsOwnValue(t *testing.T) {
	original := &struct{ name string }{"original"}
	value := NewHostValue(original)
	key, transaction := sendHostValue(t, value)
	transaction.commit()

	again, transaction := sendHostValue(t, value)
	if again != key || len(transaction.hostValueKeys) != 1 || hostKeyCount(key) != 2 {
		t.Fatalf("resent %d owning %v (count %d), want key %d owning one reference", again, transaction.hostValueKeys, hostKeyCount(key), key)
	}
	transaction.commit()
	t.Cleanup(func() { releaseHostValue(key, 2) })

	decoded, err := hostValueWire(key).HostValue()
	if err != nil {
		t.Fatal(err)
	}
	if decoded.Value() != any(original) || decoded.cell != value.cell {
		t.Fatalf("decoded %v, want the cell that was sent", decoded.Value())
	}
	twice, err := hostValueWire(key).HostValue()
	if err != nil || twice.cell != decoded.cell {
		t.Fatalf("a second decode is another host value: %v", err)
	}
	if !decoded.Equal(value) || !value.Equal(decoded) {
		t.Fatal("a decode of a held registration is not Equal to the value sent")
	}
	if decoded.Equal(NewHostValue(original)) {
		t.Fatal("another NewHostValue of one Go value is Equal to it")
	}
}

// A rolled-back send frees its registration, and the next send registers
// again rather than naming a key BAML never received.
func TestHostValueRollbackReregisters(t *testing.T) {
	value := NewHostValue(42)
	first, transaction := sendHostValue(t, value)
	transaction.rollback()
	if _, live := lookupHostObject(first); live {
		t.Fatal("rollback left the registration live")
	}
	second, retry := sendHostValue(t, value)
	if second == first || len(retry.hostValueKeys) != 1 {
		t.Fatalf("resent %d owning %v, want a fresh registration", second, retry.hostValueKeys)
	}
	retry.rollback()
}

// A rolled-back re-send gives back only the reference it took: the one BAML
// holds stays.
func TestHostValueRollbackOfResendKeepsTheHeldReference(t *testing.T) {
	value := NewHostValue("held")
	key, transaction := sendHostValue(t, value)
	transaction.commit()
	again, transaction := sendHostValue(t, value)
	if again != key {
		t.Fatalf("resent %d, want %d", again, key)
	}
	transaction.rollback()
	if hostKeyCount(key) != 1 {
		t.Fatalf("count after rolled-back resend = %d, want 1", hostKeyCount(key))
	}
	releaseHostValue(key, 1)
	if _, live := lookupHostObject(key); live {
		t.Fatal("the engine's release left the key registered")
	}
}

// Equality is identity, not liveness: a host value is still itself after
// BAML lets it go, and sent again it is registered afresh as the same cell.
func TestHostValueEqualitySurvivesRelease(t *testing.T) {
	value := NewHostValue("kept")
	key, transaction := sendHostValue(t, value)
	transaction.commit()
	decoded, err := hostValueWire(key).HostValue()
	if err != nil {
		t.Fatal(err)
	}
	releaseHostValue(key, 1)
	if !decoded.Equal(value) {
		t.Fatal("a release broke equality")
	}
	fresh, transaction := sendHostValue(t, decoded)
	transaction.commit()
	t.Cleanup(func() { releaseHostValue(fresh, 1) })
	if fresh == key {
		t.Fatal("a released key was sent again")
	}
	back, err := hostValueWire(fresh).HostValue()
	if err != nil || !back.Equal(value) {
		t.Fatalf("the re-registered value decodes as another host value: %v", err)
	}
}

// A send racing the engine's release of the last reference either takes its
// reference first, so the key survives, or registers afresh: the key sent is
// always registered.
func TestHostValueResendRacingReleaseNeverSendsADeadKey(t *testing.T) {
	baseline := registeredHostValueCount()
	for range 500 {
		value := NewHostValue("raced")
		key, transaction := sendHostValue(t, value)
		transaction.commit()
		sent := make(chan uint64)
		var wait sync.WaitGroup
		wait.Add(1)
		go func() {
			defer wait.Done()
			releaseHostValue(key, 1)
		}()
		go func() {
			transaction := &inputTransaction{}
			encoded, err := HostValueInput(value).encodeValue(transaction)
			if err != nil {
				sent <- 0
				return
			}
			transaction.commit()
			sent <- encoded.GetHandle().GetKey()
		}()
		resent := <-sent
		wait.Wait()
		if resent == 0 || hostKeyCount(resent) != 1 {
			t.Fatalf("resent key %d with count %d, want a registered key with one reference", resent, hostKeyCount(resent))
		}
		if decoded, err := hostValueWire(resent).HostValue(); err != nil || !decoded.Equal(value) {
			t.Fatalf("the resent key does not decode to the value: %v", err)
		}
		releaseHostValue(resent, 1)
	}
	if got := registeredHostValueCount(); got != baseline {
		t.Fatalf("registry = %d after the race, want %d", got, baseline)
	}
}

// A host key the engine sent Go is sent back with one more reference, which a
// rollback gives back; a key no longer registered is refused.
func TestReceivedHostKeyIsResentWithAReference(t *testing.T) {
	_, key := registerHostCell("received")
	handle := &cffi.BamlOutboundHandle{Key: key, HandleType: cffi.BamlHandleType_HOST_VALUE_OPAQUE}
	transaction := &inputTransaction{}
	encoded, err := handleToInbound(handle, transaction)
	if err != nil || encoded.GetHandle().GetKey() != key {
		t.Fatalf("resent %v, %v", encoded, err)
	}
	if hostKeyCount(key) != 2 {
		t.Fatalf("count after resend = %d, want 2", hostKeyCount(key))
	}
	transaction.rollback()
	if hostKeyCount(key) != 1 {
		t.Fatalf("count after rollback = %d, want 1", hostKeyCount(key))
	}
	releaseHostValue(key, 1)
	if _, err := handleToInbound(handle, &inputTransaction{}); err == nil {
		t.Fatal("a key no longer registered was sent")
	}
}

// A result owns the pin of each host value it carries and releases it once;
// the host value decodes to the cell that was sent.
func TestResultReleasesHostValuePinOnce(t *testing.T) {
	released := installOutboundReleaseRecorder(t)
	value := NewHostValue("pinned")
	key, transaction := sendHostValue(t, value)
	transaction.commit()
	t.Cleanup(func() { releaseHostValue(key, 1) })
	result, err := decodeResultEnvelope(&cffi.BamlOutboundResult{Result: &cffi.BamlOutboundResult_Ok{Ok: &cffi.BamlOutboundValue{
		Value: &cffi.BamlOutboundValue_HandleValue{HandleValue: &cffi.BamlOutboundHandle{
			Key: key, HandleType: cffi.BamlHandleType_HOST_VALUE_OPAQUE, Pin: 77,
		}},
	}}})
	if err != nil {
		t.Fatal(err)
	}
	decoded, err := result.HostValue()
	if err != nil || decoded.cell != value.cell {
		t.Fatalf("decoded %v, %v; want the cell that was sent", decoded, err)
	}
	releaseResultOwner(result.owner)
	releaseResultOwner(result.owner)
	if !reflect.DeepEqual(*released, []uint64{77}) {
		t.Fatalf("pins released = %v, want [77]", *released)
	}
}

func TestHostValueDecodeRejectsForeignAndDeadHandles(t *testing.T) {
	if _, err := hostValueWire(1 << 62).HostValue(); err == nil {
		t.Fatal("a key this process never registered must be rejected")
	}
	engine := handleWireValue(&cffi.BamlOutboundHandle{Key: 1, HandleType: cffi.BamlHandleType_ADT_RUNTIME_VALUE})
	if _, err := engine.HostValue(); err == nil {
		t.Fatal("an engine handle must be rejected")
	}
	if _, err := (Value{}).HostValue(); err == nil {
		t.Fatal("an uninitialized Value must be rejected")
	}
	if input := HostValueInput(HostValue{}); input.err == nil {
		t.Fatal("the zero HostValue must be rejected")
	}
}

func TestHostValueDescriptorIsTheHostValueClass(t *testing.T) {
	if !HostValueBAMLType().Equal(ClassBAMLType("baml.host.Value")) {
		t.Fatal("HostValue states another type")
	}
}
