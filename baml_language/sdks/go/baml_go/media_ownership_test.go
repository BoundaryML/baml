package baml_go

import (
	"errors"
	"reflect"
	"runtime"
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
)

func outboundMediaHandle(key uint64) *cffi.BamlOutboundValue {
	return &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_HandleValue{HandleValue: &cffi.BamlOutboundHandle{
		Key: key, HandleType: cffi.BamlHandleType_ADT_MEDIA_IMAGE,
	}}}
}

func installOutboundReleaseRecorder(t *testing.T) *[]uint64 {
	t.Helper()
	released := &[]uint64{}
	previous := releaseOutboundHandle
	releaseOutboundHandle = func(key uint64) { *released = append(*released, key) }
	t.Cleanup(func() { releaseOutboundHandle = previous })
	return released
}

func installMediaReleaseRecorder(t *testing.T) *[]uint64 {
	t.Helper()
	released := &[]uint64{}
	previous := releaseMediaHandle
	releaseMediaHandle = func(key uint64) { *released = append(*released, key) }
	t.Cleanup(func() { releaseMediaHandle = previous })
	return released
}

func TestCopiedMediaValuesShareOneHandleCellAndReleaseExactlyOnce(t *testing.T) {
	released := installMediaReleaseRecorder(t)
	handle := &mediaHandle{key: 31, handleType: cffi.BamlHandleType_ADT_MEDIA_IMAGE}
	runtime.SetFinalizer(handle, finalizeMediaHandle)
	original := Image{media: mediaValue{handle: handle, kind: mediaKindImage}}
	copyOne := original
	copyTwo := copyOne

	if original.media.handle != copyOne.media.handle || copyOne.media.handle != copyTwo.media.handle {
		t.Fatal("copied media values did not share their owned handle cell")
	}
	runtime.SetFinalizer(handle, nil)
	finalizeMediaHandle(original.media.handle)
	finalizeMediaHandle(copyOne.media.handle)
	finalizeMediaHandle(copyTwo.media.handle)
	if !reflect.DeepEqual(*released, []uint64{31}) {
		t.Fatalf("released constructor key = %v, want [31] exactly once", *released)
	}
	if original.media.handle.key != 0 || copyOne.media.handle.key != 0 || copyTwo.media.handle.key != 0 {
		t.Fatal("finalized copied media retained a live key")
	}
}

func TestClonedMediaHandleCopiesReleaseCloneKeyExactlyOnce(t *testing.T) {
	released := installMediaReleaseRecorder(t)
	previousValidate := validateOutboundMediaHandle
	previousClone := cloneOutboundMediaHandle
	validateOutboundMediaHandle = func(uint64, cffi.BamlHandleType) error { return nil }
	cloneOutboundMediaHandle = func(uint64) (uint64, error) { return 132, nil }
	t.Cleanup(func() {
		validateOutboundMediaHandle = previousValidate
		cloneOutboundMediaHandle = previousClone
	})

	media, err := mediaFromOwnedHandle(32, cffi.BamlHandleType_ADT_MEDIA_IMAGE, mediaKindImage)
	if err != nil {
		t.Fatal(err)
	}
	copyOne := media
	copyTwo := copyOne
	runtime.SetFinalizer(media.handle, nil)
	finalizeMediaHandle(media.handle)
	finalizeMediaHandle(copyOne.handle)
	finalizeMediaHandle(copyTwo.handle)
	if !reflect.DeepEqual(*released, []uint64{132}) {
		t.Fatalf("released clone key = %v, want [132] exactly once", *released)
	}
}

func TestMediaValidationAndCloneFailuresInstallNoLiveHandle(t *testing.T) {
	released := installMediaReleaseRecorder(t)
	previousValidate := validateOutboundMediaHandle
	previousClone := cloneOutboundMediaHandle
	t.Cleanup(func() {
		validateOutboundMediaHandle = previousValidate
		cloneOutboundMediaHandle = previousClone
	})

	cloneCalls := 0
	validateOutboundMediaHandle = func(uint64, cffi.BamlHandleType) error { return errors.New("wrong native kind") }
	cloneOutboundMediaHandle = func(uint64) (uint64, error) {
		cloneCalls++
		return 0, errors.New("must not clone")
	}
	media, err := mediaFromOwnedHandle(33, cffi.BamlHandleType_ADT_MEDIA_IMAGE, mediaKindImage)
	if err == nil || media.handle != nil || cloneCalls != 0 {
		t.Fatalf("validation failure = %#v, %v; clone calls = %d", media, err, cloneCalls)
	}

	validateOutboundMediaHandle = func(uint64, cffi.BamlHandleType) error { return nil }
	cloneOutboundMediaHandle = func(uint64) (uint64, error) {
		cloneCalls++
		return 0, errors.New("clone failed")
	}
	media, err = mediaFromOwnedHandle(34, cffi.BamlHandleType_ADT_MEDIA_IMAGE, mediaKindImage)
	if err == nil || media.handle != nil || cloneCalls != 1 {
		t.Fatalf("clone failure = %#v, %v; clone calls = %d", media, err, cloneCalls)
	}
	if len(*released) != 0 {
		t.Fatalf("failed decode installed/released unexpected live keys: %v", *released)
	}
}

func TestSuccessfulResultOwnerFinalizerReleasesEveryHandleExactlyOnce(t *testing.T) {
	released := installOutboundReleaseRecorder(t)
	value, err := decodeResultEnvelope(&cffi.BamlOutboundResult{Result: &cffi.BamlOutboundResult_Ok{Ok: outboundMediaHandle(41)}})
	if err != nil || value.owner == nil {
		t.Fatalf("decode = %#v, %v", value, err)
	}
	runtime.SetFinalizer(value.owner, nil)
	finalizeResultOwner(value.owner)
	finalizeResultOwner(value.owner)
	if !reflect.DeepEqual(*released, []uint64{41}) {
		t.Fatalf("released keys = %v, want [41] exactly once", *released)
	}
}

func TestFailureResultArmsReleaseHandlesDeterministically(t *testing.T) {
	released := installOutboundReleaseRecorder(t)
	previousExit := processExit
	processExit = func(int) {}
	t.Cleanup(func() { processExit = previousExit })

	results := []*cffi.BamlOutboundResult{
		{Result: &cffi.BamlOutboundResult_Error{Error: &cffi.BamlOutboundError{Value: outboundMediaHandle(51)}}},
		{Result: &cffi.BamlOutboundResult_Panic{Panic: &cffi.BamlOutboundPanic{Value: outboundMediaHandle(52)}}},
		{Result: &cffi.BamlOutboundResult_Panic{Panic: &cffi.BamlOutboundPanic{Value: outboundMediaHandle(53), IsExitPanic: true}}},
	}
	for _, result := range results {
		_, _ = decodeResultEnvelope(result)
	}
	if !reflect.DeepEqual(*released, []uint64{51, 52, 53}) {
		t.Fatalf("released keys = %v, want deterministic error/panic/exit cleanup", *released)
	}
}

func TestRepeatedMediaDecodeClonesIndependentlyWhileOriginalOwnerLives(t *testing.T) {
	released := installOutboundReleaseRecorder(t)
	previousValidate := validateOutboundMediaHandle
	previousClone := cloneOutboundMediaHandle
	validateOutboundMediaHandle = func(uint64, cffi.BamlHandleType) error { return nil }
	next := uint64(100)
	cloneOutboundMediaHandle = func(key uint64) (uint64, error) {
		if key != 61 {
			return 0, errors.New("unexpected source key")
		}
		next++
		return next, nil
	}
	t.Cleanup(func() {
		validateOutboundMediaHandle = previousValidate
		cloneOutboundMediaHandle = previousClone
	})

	value, err := decodeResultEnvelope(&cffi.BamlOutboundResult{Result: &cffi.BamlOutboundResult_Ok{Ok: outboundMediaHandle(61)}})
	if err != nil {
		t.Fatal(err)
	}
	first, err := value.Image()
	if err != nil {
		t.Fatal(err)
	}
	second, err := value.Image()
	if err != nil {
		t.Fatal(err)
	}
	if first.media.handle.key != 101 || second.media.handle.key != 102 {
		t.Fatalf("clone keys = %d, %d", first.media.handle.key, second.media.handle.key)
	}
	if len(*released) != 0 || !reflect.DeepEqual(value.owner.keys, []uint64{61}) {
		t.Fatalf("original owner released early: keys=%v releases=%v", value.owner.keys, *released)
	}
	runtime.SetFinalizer(first.media.handle, nil)
	runtime.SetFinalizer(second.media.handle, nil)
	runtime.SetFinalizer(value.owner, nil)
	finalizeResultOwner(value.owner)
	if !reflect.DeepEqual(*released, []uint64{61}) {
		t.Fatalf("original release = %v", *released)
	}
}

func TestNestedDecodersPropagateOneOwnerThroughEveryShape(t *testing.T) {
	leaf := outboundMediaHandle(71)
	owner := &resultOwner{keys: []uint64{71}}
	assertOwner := func(value Value) {
		t.Helper()
		if value.owner != owner {
			t.Fatalf("nested value owner = %p, want %p", value.owner, owner)
		}
	}

	list := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{ListValue: &cffi.BamlValueList{Items: []*cffi.BamlOutboundValue{leaf}}}}, owner: owner}
	if _, err := DecodeList(list, func(value Value) (struct{}, error) { assertOwner(value); return struct{}{}, nil }); err != nil {
		t.Fatal(err)
	}
	mapValue := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_MapValue{MapValue: &cffi.BamlValueMap{Entries: []*cffi.BamlOutboundMapEntry{{Key: "media", Value: leaf}}}}}, owner: owner}
	if _, err := DecodeMap(mapValue, func(value Value) (struct{}, error) { assertOwner(value); return struct{}{}, nil }); err != nil {
		t.Fatal(err)
	}
	classValue := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{Name: "user.Box", Fields: []*cffi.BamlOutboundMapEntry{{Key: "media", Value: leaf}}}}}, owner: owner}
	class, err := classValue.Class("user.Box")
	if err != nil {
		t.Fatal(err)
	}
	field, err := class.Field("media")
	if err != nil {
		t.Fatal(err)
	}
	assertOwner(field)

	union := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_UnionVariantValue{UnionVariantValue: &cffi.BamlValueUnionVariant{
		SelfType:            UnionBAMLType(ImageBAMLType(), PrimitiveBAMLType(StringType)).value,
		SelectedOptionIndex: uint32Pointer(0),
		Value:               leaf,
	}}}, owner: owner}
	_, payload, err := union.UnionVariant()
	if err != nil {
		t.Fatal(err)
	}
	assertOwner(payload)
}

func TestDecoderErrorLeavesOwnerAvailableForExactCleanup(t *testing.T) {
	released := installOutboundReleaseRecorder(t)
	value, err := decodeResultEnvelope(&cffi.BamlOutboundResult{Result: &cffi.BamlOutboundResult_Ok{Ok: &cffi.BamlOutboundValue{
		Value: &cffi.BamlOutboundValue_ListValue{ListValue: &cffi.BamlValueList{Items: []*cffi.BamlOutboundValue{outboundMediaHandle(81)}}},
	}}})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := DecodeList(value, func(Value) (string, error) { return "", errors.New("decoder failed") }); err == nil {
		t.Fatal("nested decoder unexpectedly succeeded")
	}
	runtime.SetFinalizer(value.owner, nil)
	finalizeResultOwner(value.owner)
	if !reflect.DeepEqual(*released, []uint64{81}) {
		t.Fatalf("decoder-error cleanup = %v", *released)
	}
}
