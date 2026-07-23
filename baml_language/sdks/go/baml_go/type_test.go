package baml_go

import (
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
)

func TestReflectedTypeInputOutputAndAny(t *testing.T) {
	want := ClassBAMLType("example.Box", PrimitiveBAMLType(IntType))
	encoded := Type(want)
	if encoded.err != nil || encoded.value.GetTyValue() == nil {
		t.Fatalf("Type() = %#v, %v", encoded.value, encoded.err)
	}

	dynamic := Any(want)
	if dynamic.err != nil || dynamic.value.GetTyValue() == nil {
		t.Fatalf("Any(BAMLType) = %#v, %v", dynamic.value, dynamic.err)
	}

	wire := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_TyValue{TyValue: want.value}}
	got, err := (Value{value: wire}).Type()
	if err != nil || !got.Equal(want) {
		t.Fatalf("Value.Type() = %#v, %v", got, err)
	}
	wire.GetTyValue().GetClassTy().Name = "mutated.after.decode"
	if !got.Equal(ClassBAMLType("example.Box", PrimitiveBAMLType(IntType))) {
		t.Fatal("decoded BAMLType retained mutable wire storage")
	}
}

func TestReflectedTypeDescriptorsComposeAndCompareExactly(t *testing.T) {
	integer := PrimitiveBAMLType(IntType)
	optional := OptionalBAMLType(integer)
	if integer.Equal(optional) {
		t.Fatal("exact equality erased top-level optionality")
	}
	if !integer.MatchesUnionArm(optional) {
		t.Fatal("union-arm matching did not tolerate the legacy top-level optional wrapper")
	}
	left := UnionBAMLType(integer, PrimitiveBAMLType(StringType))
	right := UnionBAMLType(PrimitiveBAMLType(StringType), integer)
	if !left.Equal(right) {
		t.Fatal("union equality depends on source order")
	}
	null := PrimitiveBAMLType(NullType)
	if !optional.Equal(UnionBAMLType(integer, null)) {
		t.Fatal("optional descriptor did not equal its structural union form")
	}
	if !OptionalBAMLType(optional).Equal(optional) {
		t.Fatal("nested optional descriptor did not collapse semantically")
	}
	if integer.Equal(UnionBAMLType(integer, null)) {
		t.Fatal("non-null descriptor compared equal to its nullable form")
	}

	list := Any([]BAMLType{})
	if list.err != nil || list.value.GetListValue() == nil || list.value.GetValueType() == nil {
		t.Fatalf("empty []BAMLType lost its static descriptor: %#v, %v", list.value, list.err)
	}
	if !(BAMLType{value: list.value.GetValueType()}).Equal(ListBAMLType(MetaTypeBAMLType())) {
		t.Fatal("[]BAMLType value descriptor is not a list of the BAML metatype")
	}
}

func TestReflectedTypeRejectsMalformedDescriptors(t *testing.T) {
	tests := []BAMLType{
		{},
		{value: &cffi.BamlTy{}},
		{value: &cffi.BamlTy{Ty: &cffi.BamlTy_List{List: &cffi.BamlTyList{}}}},
		{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Union{Union: &cffi.BamlTyUnion{}}}},
		{value: &cffi.BamlTy{Ty: &cffi.BamlTy_MetaType{}}},
		{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Primitive{Primitive: &cffi.BamlTyPrimitive{Kind: 999}}}},
		BigintLiteralBAMLType("not-an-integer"),
		FloatLiteralBAMLType("NaN"),
		{value: &cffi.BamlTy{Ty: &cffi.BamlTy_TypeAlias{TypeAlias: &cffi.BamlTyTypeAlias{
			Name: "example.Alias", TypeArgs: []*cffi.BamlTy{PrimitiveBAMLType(IntType).value},
		}}}},
		{value: &cffi.BamlTy{Ty: &cffi.BamlTy_AssociatedTypeProjection{AssociatedTypeProjection: &cffi.BamlTyAssociatedTypeProjection{
			Base: PrimitiveBAMLType(IntType).value, Member: "Item",
		}}}},
		{value: &cffi.BamlTy{Ty: &cffi.BamlTy_AssociatedTypeProjection{AssociatedTypeProjection: &cffi.BamlTyAssociatedTypeProjection{
			Base: PrimitiveBAMLType(IntType).value, Interface: PrimitiveBAMLType(IntType).value, Member: "Item",
		}}}},
	}
	for index, malformed := range tests {
		if input := Type(malformed); input.err == nil {
			t.Fatalf("malformed descriptor %d was accepted", index)
		}
	}

	malformedOutput := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_TyValue{
		TyValue: &cffi.BamlTy{Ty: &cffi.BamlTy_Optional{Optional: &cffi.BamlTyOptional{}}},
	}}}
	if _, err := malformedOutput.Type(); err == nil {
		t.Fatal("malformed outbound descriptor was accepted")
	}
}
