package baml_go

import (
	"errors"
	"reflect"
	"runtime"
	"sort"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/boundaryml/baml-go/internal/cffi"
	"google.golang.org/protobuf/proto"
)

func marshalHostDispatch(t *testing.T, args ...*cffi.BamlToHostArg) []byte {
	t.Helper()
	payload, err := proto.Marshal(&cffi.BamlToHostCall{Args: args})
	if err != nil {
		t.Fatal(err)
	}
	return payload
}

func cleanupCompletedHostError(payload []byte) {
	var inbound cffi.InboundValue
	if proto.Unmarshal(payload, &inbound) != nil {
		return
	}
	class := inbound.GetClassValue()
	if class == nil {
		return
	}
	for _, field := range class.Fields {
		if field != nil && field.GetStringKey() == "_handle" && field.GetValue() != nil {
			if handle := field.GetValue().GetHandle(); handle != nil {
				unregisterHostValue(handle.Key)
			}
		}
	}
}

func registeredHostValueCount() int {
	hostValues.Lock()
	defer hostValues.Unlock()
	return len(hostValues.table)
}

func outboundHostCallableFailure(key uint64, extra *cffi.BamlOutboundValue) *cffi.BamlOutboundValue {
	fields := []*cffi.BamlOutboundMapEntry{
		{Key: "message", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "native callback failed"}}},
		{Key: "_handle", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_HandleValue{HandleValue: &cffi.BamlOutboundHandle{Key: key, HandleType: cffi.BamlHandleType_HOST_VALUE_OPAQUE}}}},
	}
	if extra != nil {
		fields = append(fields, &cffi.BamlOutboundMapEntry{Key: "other", Value: extra})
	}
	return &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{
		Name: "baml.errors.HostCallable", Fields: fields,
	}}}
}

func TestHostCallableTransactionRollbackAndRuntimeReleaseAreExact(t *testing.T) {
	input := HostCallable(func(HostCallArguments) (Input, error) { return String("ok"), nil })
	transaction := &inputTransaction{}
	encoded, err := input.encodeValue(transaction)
	if err != nil {
		t.Fatal(err)
	}
	handle := encoded.GetHandle()
	if handle == nil || handle.HandleType != cffi.BamlHandleType_HOST_VALUE_CALLABLE || handle.Key == 0 {
		t.Fatalf("host callable handle = %#v", handle)
	}
	if _, ok := lookupHostCallable(handle.Key); !ok {
		t.Fatal("registered callback is missing before rollback")
	}
	transaction.rollback()
	if _, ok := lookupHostCallable(handle.Key); ok {
		t.Fatal("rollback leaked registered callback")
	}

	transaction = &inputTransaction{}
	encoded, err = input.encodeValue(transaction)
	if err != nil {
		t.Fatal(err)
	}
	handle = encoded.GetHandle()
	transaction.commitHostValues()
	transaction.rollback()
	if _, ok := lookupHostCallable(handle.Key); !ok {
		t.Fatal("committed callback was released by transaction rollback")
	}
	unregisterHostValue(handle.Key)
	unregisterHostValue(handle.Key)
	if _, ok := lookupHostCallable(handle.Key); ok {
		t.Fatal("runtime release did not remove callback")
	}
}

func TestHostCallableRegistrationIsConcurrent(t *testing.T) {
	const count = 64
	keys := make(chan uint64, count)
	var wait sync.WaitGroup
	for range count {
		wait.Add(1)
		go func() {
			defer wait.Done()
			keys <- registerHostValue(hostValue{callable: func(HostCallArguments) (Input, error) {
				return NullInput(Null{}), nil
			}})
		}()
	}
	wait.Wait()
	close(keys)
	seen := make(map[uint64]struct{}, count)
	for key := range keys {
		if key == 0 {
			t.Fatal("registered zero host-value key")
		}
		if _, duplicate := seen[key]; duplicate {
			t.Fatalf("duplicate host-value key %d", key)
		}
		seen[key] = struct{}{}
		unregisterHostValue(key)
	}
}

func TestNilHostCallableDoesNotRegister(t *testing.T) {
	baseline := registeredHostValueCount()
	transaction := &inputTransaction{}
	if _, err := HostCallable(nil).encodeValue(transaction); err == nil {
		t.Fatal("nil host callable unexpectedly encoded")
	}
	transaction.rollback()
	if got := registeredHostValueCount(); got != baseline {
		t.Fatalf("nil host callable changed registry size: got %d, want %d", got, baseline)
	}
}

func TestMalformedHostDispatchCompletesWithStructuredError(t *testing.T) {
	previous := completeNativeHostCall
	t.Cleanup(func() { completeNativeHostCall = previous })
	type completion struct {
		isError bool
		payload []byte
	}
	completed := make(chan completion, 2)
	completeNativeHostCall = func(_ uint32, isError bool, payload []byte) {
		completed <- completion{isError: isError, payload: append([]byte(nil), payload...)}
	}

	dispatchHostCall(999_999, 1, nil)
	assertHostCallableFailure(t, <-completed)

	key := registerHostValue(hostValue{callable: func(HostCallArguments) (Input, error) {
		return String("unused"), nil
	}})
	t.Cleanup(func() { unregisterHostValue(key) })
	dispatchHostCall(key, 2, []byte{0xff})
	assertHostCallableFailure(t, <-completed)
}

func TestHostDispatchPartitionsRequiredAndOptionalArguments(t *testing.T) {
	previous := completeNativeHostCall
	t.Cleanup(func() { completeNativeHostCall = previous })
	completed := make(chan bool, 1)
	completeNativeHostCall = func(_ uint32, isError bool, _ []byte) { completed <- isError }

	observed := make(chan HostCallArguments, 1)
	key := registerHostValue(hostValue{callable: func(arguments HostCallArguments) (Input, error) {
		observed <- arguments
		return String("ok"), nil
	}})
	t.Cleanup(func() { unregisterHostValue(key) })
	dispatchHostCall(key, 1, marshalHostDispatch(t,
		&cffi.BamlToHostArg{Value: outboundString("required")},
		&cffi.BamlToHostArg{Value: outboundString("optional"), IsOptionalArg: true, ArgName: "named"},
	))

	select {
	case arguments := <-observed:
		if arguments.RequiredCount() != 1 || arguments.OptionalCount() != 1 {
			t.Fatalf("argument counts = (%d, %d), want (1, 1)", arguments.RequiredCount(), arguments.OptionalCount())
		}
		if _, ok := arguments.Optional("named"); !ok {
			t.Fatal("supplied optional argument is absent")
		}
		if _, ok := arguments.Optional("omitted"); ok {
			t.Fatal("omitted optional argument is present")
		}
	case <-time.After(time.Second):
		t.Fatal("callback did not observe dispatched arguments")
	}
	if isError := <-completed; isError {
		t.Fatal("valid required/optional dispatch completed with an error")
	}
}

func assertHostCallableFailure(t *testing.T, completed struct {
	isError bool
	payload []byte
}) {
	t.Helper()
	if !completed.isError {
		t.Fatal("malformed host dispatch completed successfully")
	}
	var inbound cffi.InboundValue
	if err := proto.Unmarshal(completed.payload, &inbound); err != nil {
		t.Fatalf("decode host failure: %v", err)
	}
	class := inbound.GetClassValue()
	if class == nil || inbound.GetValueType().GetClassTy().GetName() != "baml.errors.HostCallable" {
		t.Fatalf("host failure class = %#v", class)
	}
	for _, field := range class.Fields {
		if field.GetStringKey() == "message" && strings.TrimSpace(field.GetValue().GetStringValue()) != "" {
			return
		}
	}
	t.Fatal("host failure has no message")
}

func TestHostCallablePanicAndReturnedErrorCompleteOnce(t *testing.T) {
	baseline := registeredHostValueCount()
	previous := completeNativeHostCall
	t.Cleanup(func() { completeNativeHostCall = previous })
	completed := make(chan bool, 3)
	completeNativeHostCall = func(_ uint32, isError bool, payload []byte) {
		cleanupCompletedHostError(payload)
		completed <- isError
	}

	runHostCall(1, func(HostCallArguments) (Input, error) { panic("boom") }, HostCallArguments{}, nil)
	runHostCall(2, func(HostCallArguments) (Input, error) { return Input{}, errors.New("failed") }, HostCallArguments{}, nil)
	go runHostCall(3, func(HostCallArguments) (Input, error) {
		runtime.Goexit()
		return String("unreachable"), nil
	}, HostCallArguments{}, nil)
	for range 3 {
		select {
		case isError := <-completed:
			if !isError {
				t.Fatal("callback failure completed successfully")
			}
		case <-time.After(time.Second):
			t.Fatal("callback failure did not complete")
		}
	}
	select {
	case <-completed:
		t.Fatal("callback completed more than once")
	default:
	}
	if got := registeredHostValueCount(); got != baseline {
		t.Fatalf("callback failures leaked host values: got %d, want baseline %d", got, baseline)
	}
}

func TestRejectedHostDispatchReleasesEveryNativeHandleExactlyOnce(t *testing.T) {
	released := installOutboundReleaseRecorder(t)
	previous := completeNativeHostCall
	completed := make(chan struct{}, 1)
	completeNativeHostCall = func(_ uint32, _ bool, payload []byte) {
		cleanupCompletedHostError(payload)
		completed <- struct{}{}
	}
	t.Cleanup(func() { completeNativeHostCall = previous })

	callableKey := registerHostValue(hostValue{callable: func(arguments HostCallArguments) (Input, error) {
		if arguments.RequiredCount() != 1 {
			return InvalidInput("bad arity"), HostCallableArityError(1, arguments.RequiredCount())
		}
		return String("ok"), nil
	}})
	t.Cleanup(func() { unregisterHostValue(callableKey) })
	handle := func(key uint64) *cffi.BamlToHostArg {
		return &cffi.BamlToHostArg{Value: outboundMediaHandle(key)}
	}

	tests := []struct {
		name string
		key  uint64
		args []*cffi.BamlToHostArg
		want []uint64
	}{
		{name: "unknown callable deduplicates handles", key: ^uint64(0), args: []*cffi.BamlToHostArg{handle(101), handle(101)}, want: []uint64{101}},
		{name: "empty argument releases later handles", key: callableKey, args: []*cffi.BamlToHostArg{{}, handle(102)}, want: []uint64{102}},
		{name: "unnamed optional metadata", key: callableKey, args: []*cffi.BamlToHostArg{{Value: outboundMediaHandle(103), IsOptionalArg: true}}, want: []uint64{103}},
		{name: "named metadata", key: callableKey, args: []*cffi.BamlToHostArg{{Value: outboundMediaHandle(104), ArgName: "value"}}, want: []uint64{104}},
		{name: "adapter arity", key: callableKey, args: []*cffi.BamlToHostArg{handle(105), handle(106)}, want: []uint64{105, 106}},
		{name: "duplicate optional name", key: callableKey, args: []*cffi.BamlToHostArg{{Value: outboundMediaHandle(108), IsOptionalArg: true, ArgName: "value"}, {Value: outboundMediaHandle(109), IsOptionalArg: true, ArgName: "value"}}, want: []uint64{108, 109}},
		{name: "required after optional", key: callableKey, args: []*cffi.BamlToHostArg{{Value: outboundMediaHandle(110), IsOptionalArg: true, ArgName: "value"}, handle(111)}, want: []uint64{110, 111}},
	}
	for index, test := range tests {
		*released = nil
		dispatchHostCall(test.key, uint32(index+1), marshalHostDispatch(t, test.args...))
		select {
		case <-completed:
		case <-time.After(time.Second):
			t.Fatalf("%s: dispatch did not complete", test.name)
		}
		got := append([]uint64(nil), (*released)...)
		sort.Slice(got, func(i, j int) bool { return got[i] < got[j] })
		if !reflect.DeepEqual(got, test.want) {
			t.Fatalf("%s: native releases = %v, want %v", test.name, got, test.want)
		}
	}

	*released = nil
	partial := append(marshalHostDispatch(t, handle(107)), 0xff)
	dispatchHostCall(callableKey, 99, partial)
	select {
	case <-completed:
	case <-time.After(time.Second):
		t.Fatal("partial protobuf dispatch did not complete")
	}
	if !reflect.DeepEqual(*released, []uint64{107}) {
		t.Fatalf("partial protobuf native releases = %v, want [107]", *released)
	}
}

func TestFailureResultReleasesOnlyOpaqueHostIdentityAfterFormatting(t *testing.T) {
	baseline := registeredHostValueCount()
	opaqueKey := registerHostValue(hostValue{opaque: errors.New("native callback failed")})
	callableKey := registerHostValue(hostValue{callable: func(HostCallArguments) (Input, error) {
		return String("still live"), nil
	}})
	callableHandle := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_HandleValue{HandleValue: &cffi.BamlOutboundHandle{
		Key: callableKey, HandleType: cffi.BamlHandleType_HOST_VALUE_CALLABLE,
	}}}
	value := outboundHostCallableFailure(opaqueKey, callableHandle)
	_, err := decodeResultEnvelope(&cffi.BamlOutboundResult{Result: &cffi.BamlOutboundResult_Error{Error: &cffi.BamlOutboundError{Value: value}}})
	if err == nil || !strings.Contains(err.Error(), "native callback failed") {
		t.Fatalf("formatted callback failure = %v", err)
	}
	if got := registeredHostValueCount(); got != baseline+1 {
		t.Fatalf("registry count after failure formatting = %d, want %d", got, baseline+1)
	}
	if _, ok := lookupHostCallable(callableKey); !ok {
		t.Fatal("failure cleanup incorrectly released a callable handle")
	}
	unregisterHostValue(callableKey)
	if got := registeredHostValueCount(); got != baseline {
		t.Fatalf("registry did not return to baseline: got %d, want %d", got, baseline)
	}
}
