package baml_go

import (
	"errors"
	"reflect"
	"runtime"
	"strings"
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
)

func liveClassTy(key uint64) *cffi.BamlTy {
	return &cffi.BamlTy{Ty: &cffi.BamlTy_ClassTy{ClassTy: &cffi.BamlTyClass{Head: &cffi.BamlTyClass_Live{Live: key}}}}
}

func labelledClassTy(label string) *cffi.BamlTy {
	return &cffi.BamlTy{Ty: &cffi.BamlTy_ClassTy{ClassTy: &cffi.BamlTyClass{Head: &cffi.BamlTyClass_AnonymousLabel{AnonymousLabel: label}}}}
}

// installTypeRowLifecycle records row clones (each returning a fresh key from
// 500) and releases.
func installTypeRowLifecycle(t *testing.T) (released *[]uint64) {
	t.Helper()
	previousClone, previousRelease := cloneOutboundTypeRow, releaseTypeRow
	next := uint64(500)
	cloneOutboundTypeRow = func(uint64) (uint64, error) {
		next++
		return next, nil
	}
	released = &[]uint64{}
	releaseTypeRow = func(key uint64) { *released = append(*released, key) }
	t.Cleanup(func() { cloneOutboundTypeRow, releaseTypeRow = previousClone, previousRelease })
	return released
}

func returnedType(t *testing.T, ty *cffi.BamlTy) BAMLType {
	t.Helper()
	adopted, err := (Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_TyValue{TyValue: ty}}}).Type()
	if err != nil {
		t.Fatal(err)
	}
	if adopted.rows != nil {
		runtime.SetFinalizer(adopted.rows, nil)
	}
	return adopted
}

func TestReturnedTypeOwnsOneReferencePerLiveHead(t *testing.T) {
	released := installTypeRowLifecycle(t)
	// One runtime declaration occurring twice arrives as one key carrying two
	// references.
	wire := UnionBAMLType(BAMLType{value: liveClassTy(7)}, ListBAMLType(BAMLType{value: liveClassTy(7)})).value
	adopted := returnedType(t, wire)
	if adopted.rows == nil || len(adopted.rows.keys) != 2 {
		t.Fatalf("adopted rows = %#v, want two references", adopted.rows)
	}
	if got := liveHeadKeys(adopted.value); len(got) != 2 || got[0] == 7 || got[1] == 7 {
		t.Fatalf("adopted heads = %v, want Go's own clones", got)
	}
	if got := liveHeadKeys(wire); !reflect.DeepEqual(got, []uint64{7, 7}) {
		t.Fatalf("adoption mutated the result's tree: %v", got)
	}
	finalizeLiveTypeRows(adopted.rows)
	if len(*released) != 2 {
		t.Fatalf("released = %v, want both references", *released)
	}
}

func TestReturnedTypeAdoptionFailureReleasesItsClones(t *testing.T) {
	released := installTypeRowLifecycle(t)
	calls := 0
	cloneOutboundTypeRow = func(uint64) (uint64, error) {
		calls++
		if calls == 2 {
			return 0, errors.New("injected clone failure")
		}
		return 600, nil
	}
	wire := UnionBAMLType(BAMLType{value: liveClassTy(7)}, BAMLType{value: liveClassTy(8)}).value
	_, err := (Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_TyValue{TyValue: wire}}}).Type()
	if err == nil || !reflect.DeepEqual(*released, []uint64{600}) {
		t.Fatalf("error = %v, released = %v; want failure releasing [600]", err, *released)
	}
}

func TestOutboundWalkerOwnsLiveHeadsOfTypeValues(t *testing.T) {
	wire := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{ListValue: &cffi.BamlValueList{
		ItemType: MetaTypeBAMLType().value,
		Items: []*cffi.BamlOutboundValue{
			{Value: &cffi.BamlOutboundValue_TyValue{TyValue: ListBAMLType(BAMLType{value: liveClassTy(9)}).value}},
			{Value: &cffi.BamlOutboundValue_TyValue{TyValue: liveClassTy(9)}},
		},
	}}}
	if keys := appendOutboundHandles(nil, wire); !reflect.DeepEqual(keys, []uint64{9, 9}) {
		t.Fatalf("owned keys = %v, want [9 9]", keys)
	}
}

func TestStatingALiveTypeClonesEveryOccurrenceIntoTheTransaction(t *testing.T) {
	installTypeRowLifecycle(t)
	lifecycle := installInputHandleLifecycle(t)
	live := returnedType(t, liveClassTy(7))
	held := liveHeadKeys(live.value)[0]

	input := Type(UnionBAMLType(live, live.Array()))
	if input.deferred == nil {
		t.Fatal("a type naming a runtime declaration was encoded outside a transaction")
	}
	payload, transaction, err := encodeCallForDispatchWithTypeArgs(42, map[string]Input{"t": input}, []TypeArgument{{Name: "T", Type: live}})
	if err != nil {
		t.Fatal(err)
	}
	cloned, _ := lifecycle.snapshot()
	if !reflect.DeepEqual(cloned, []uint64{held, held, held}) {
		t.Fatalf("cloned = %v, want one clone of %d per occurrence", cloned, held)
	}
	if !reflect.DeepEqual(transaction.keys, []uint64{1001, 1002, 1003}) {
		t.Fatalf("journaled = %v", transaction.keys)
	}
	if len(payload) == 0 || liveHeadKeys(live.value)[0] != held {
		t.Fatal("stating the type changed the caller's own reference")
	}
	transaction.rollback()
	if _, released := lifecycle.snapshot(); !reflect.DeepEqual(released, []uint64{1001, 1002, 1003}) {
		t.Fatalf("rollback released %v, want every clone", released)
	}
}

func TestCompositionSharesLiveRows(t *testing.T) {
	installTypeRowLifecycle(t)
	first := returnedType(t, liveClassTy(7))
	second := returnedType(t, liveClassTy(8))
	if first.Array().rows != first.rows || first.Optional().Array().rows != first.rows {
		t.Fatal("Array/Optional did not share the composed type's rows")
	}
	if ClassBAMLType("user.Box", first).rows != first.rows {
		t.Fatal("a class argument's rows were not shared")
	}
	both := MapBAMLType(PrimitiveBAMLType(StringType), UnionBAMLType(first, second, first))
	if both.rows == nil || len(both.rows.keys) != 0 || !reflect.DeepEqual(both.rows.parts, []*liveTypeRows{first.rows, second.rows}) {
		t.Fatalf("composite rows = %#v, want a retaining node over both owners", both.rows)
	}
	if PrimitiveBAMLType(IntType).Array().transfers() || PrimitiveBAMLType(IntType).Array().rows != nil {
		t.Fatal("a type with no live head acquired rows")
	}
}

func TestLiveHeadsCompareByKey(t *testing.T) {
	if !(BAMLType{value: liveClassTy(7)}).Equal(BAMLType{value: liveClassTy(7)}) {
		t.Fatal("equal keys compared unequal")
	}
	if (BAMLType{value: liveClassTy(7)}).Equal(BAMLType{value: liveClassTy(8)}) {
		t.Fatal("distinct keys compared equal")
	}
}

func TestHeadsAreAdmittedByPosition(t *testing.T) {
	installTypeRowLifecycle(t)
	labelled := returnedType(t, labelledClassTy("Fresh"))
	if !labelled.Equal(BAMLType{value: labelledClassTy("Fresh")}) {
		t.Fatal("a returned anonymous label was not readable")
	}
	if input := Type(labelled); input.err == nil || !strings.Contains(input.err.Error(), "cannot be stated") {
		t.Fatalf("sending an anonymous label: %v", input.err)
	}
	if _, _, err := encodeCallForDispatchWithTypeArgs(1, nil, []TypeArgument{{Name: "T", Type: labelled.Array()}}); err == nil {
		t.Fatal("an anonymous label was sent as a type argument")
	}

	headless := &cffi.BamlTy{Ty: &cffi.BamlTy_Enum{Enum: &cffi.BamlTyEnum{}}}
	for _, position := range []typePosition{typeAnnotation, typeHeld, typeStated} {
		if err := validateBAMLType(headless, position); err == nil {
			t.Fatalf("position %d accepted a head naming nothing", position)
		}
	}

	if err := validateBAMLType(liveClassTy(7), typeAnnotation); err == nil {
		t.Fatal("an annotation carried a live row")
	}
	if err := validateBAMLType(labelledClassTy("Fresh"), typeAnnotation); err != nil {
		t.Fatalf("an annotation label was refused: %v", err)
	}
	variant := &cffi.BamlValueUnionVariant{
		SelfType:            UnionBAMLType(BAMLType{value: liveClassTy(7)}, PrimitiveBAMLType(IntType)).value,
		SelectedOptionIndex: new(uint32),
		Value:               &cffi.BamlOutboundValue{},
	}
	if _, _, err := validateUnionVariant(variant); err == nil {
		t.Fatal("a selected union arm carried a live row")
	}
}
