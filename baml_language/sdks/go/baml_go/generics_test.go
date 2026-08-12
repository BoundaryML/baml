package baml_go

import (
	"math/big"
	"strings"
	"testing"

	"google.golang.org/protobuf/proto"

	"github.com/boundaryml/baml-go/internal/cffi"
)

type genericTestBox[T any] struct {
	Value T `baml:"value"`
}

type genericTestEnum string

func (genericTestEnum) BAMLEnumName() string       { return "user.tests.Enum" }
func (genericTestEnum) BAMLEnumVariants() []string { return []string{"ONE", "TWO"} }

type genericTestNode[T any] struct {
	Value T                   `baml:"value"`
	Next  *genericTestNode[T] `baml:"next"`
}

func (genericTestNode[T]) BAMLClassName() string { return "user.tests.Node" }
func (genericTestNode[T]) BAMLType() BAMLType {
	return ClassBAMLType("user.tests.Node", TypeOf[T]())
}
func (value genericTestNode[T]) BAMLInput() Input { return GeneratedClassInput(value) }
func (genericTestNode[T]) BAMLDecode(value Value) (any, error) {
	return DecodeGeneratedClass[genericTestNode[T]](value)
}

func (genericTestBox[T]) BAMLClassName() string { return "user.tests.Box" }
func (genericTestBox[T]) BAMLType() BAMLType {
	return ClassBAMLType("user.tests.Box", TypeOf[T]())
}
func (value genericTestBox[T]) BAMLInput() Input { return GeneratedClassInput(value) }
func (genericTestBox[T]) BAMLDecode(value Value) (any, error) {
	return DecodeGeneratedClass[genericTestBox[T]](value)
}

func TestStructuredGenericTypeArgumentsReachCallWire(t *testing.T) {
	nested := ClassBAMLType("user.tests.Box", ListBAMLType(PrimitiveBAMLType(IntType)))
	payload, transaction, err := encodeCallForDispatchWithTypeArgs(9, map[string]Input{
		"value": String("ok"),
	}, []TypeArgument{{Name: "T", Type: nested}})
	if transaction != nil {
		defer transaction.rollback()
	}
	if err != nil {
		t.Fatal(err)
	}
	var call cffi.CallFunctionArgs
	if err := proto.Unmarshal(payload, &call); err != nil {
		t.Fatal(err)
	}
	if len(call.TypeArgs) != 1 || call.TypeArgs[0].TypeVar != "T" {
		t.Fatalf("type args = %#v", call.TypeArgs)
	}
	if got := (BAMLType{value: call.TypeArgs[0].TypeValue}); !got.Equal(nested) {
		t.Fatalf("type arg = %#v", got)
	}
}

func TestTypeOfCoversEverySupportedGenericLeaf(t *testing.T) {
	assert := func(name string, got, want BAMLType) {
		t.Helper()
		if !got.Equal(want) {
			t.Errorf("TypeOf[%s] = %#v, want %#v", name, got, want)
		}
	}
	stringType := PrimitiveBAMLType(StringType)
	intType := PrimitiveBAMLType(IntType)

	assert("string", TypeOf[string](), stringType)
	assert("bool", TypeOf[bool](), PrimitiveBAMLType(BoolType))
	assert("int", TypeOf[int](), intType)
	assert("int8", TypeOf[int8](), intType)
	assert("int16", TypeOf[int16](), intType)
	assert("int32", TypeOf[int32](), intType)
	assert("int64", TypeOf[int64](), intType)
	assert("uint", TypeOf[uint](), intType)
	assert("uint8", TypeOf[uint8](), intType)
	assert("uint16", TypeOf[uint16](), intType)
	assert("uint32", TypeOf[uint32](), intType)
	assert("uint64", TypeOf[uint64](), intType)
	assert("uintptr", TypeOf[uintptr](), intType)
	assert("float32", TypeOf[float32](), PrimitiveBAMLType(FloatType))
	assert("float64", TypeOf[float64](), PrimitiveBAMLType(FloatType))
	assert("*big.Int", TypeOf[*big.Int](), PrimitiveBAMLType(BigintType))
	assert("[]byte", TypeOf[[]byte](), PrimitiveBAMLType(BytesType))
	assert("Null", TypeOf[Null](), PrimitiveBAMLType(NullType))
	assert("BAMLType", TypeOf[BAMLType](), MetaTypeBAMLType())
	assert("Image", TypeOf[Image](), ImageBAMLType())
	assert("Audio", TypeOf[Audio](), AudioBAMLType())
	assert("Video", TypeOf[Video](), VideoBAMLType())
	assert("Pdf", TypeOf[Pdf](), PdfBAMLType())
	assert("RustType", TypeOf[RustType](), RustTypeBAMLType())
	assert("enum", TypeOf[genericTestEnum](), EnumBAMLType("user.tests.Enum"))
	assert(
		"class",
		TypeOf[genericTestBox[string]](),
		ClassBAMLType("user.tests.Box", stringType),
	)
}

func TestTypeOfPreservesOptionalListMapAndGenericClassNesting(t *testing.T) {
	want := MapBAMLType(
		PrimitiveBAMLType(StringType),
		ListBAMLType(OptionalBAMLType(ClassBAMLType("user.tests.Box", AudioBAMLType()))),
	)
	got := TypeOf[map[string][]*genericTestBox[Audio]]()
	if !got.Equal(want) {
		t.Fatalf("nested descriptor = %#v, want %#v", got, want)
	}

	doubleOptional := TypeOf[**int64]()
	outer := doubleOptional.value.GetOptional()
	if outer == nil || outer.Inner.GetOptional() == nil || outer.Inner.GetOptional().Inner.GetPrimitive() == nil {
		t.Fatalf("TypeOf[**int64] did not preserve both nullable boundaries: %#v", doubleOptional)
	}
}

func TestGenericAnyCannotReifyCanonicalJSONAliasTypeArgument(t *testing.T) {
	descriptor := TypeOf[any]()
	if descriptor.value != nil {
		t.Fatalf("TypeOf[any] guessed descriptor %#v", descriptor)
	}
	_, _, err := encodeCallForDispatchWithTypeArgs(1, nil, []TypeArgument{{Name: "T", Type: descriptor}})
	if err == nil || !strings.Contains(err.Error(), "descriptor") {
		t.Fatalf("TypeOf[any] call error = %v", err)
	}
	stringValue := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "json"}}}
	decoded, err := DecodeAs[any](stringValue)
	if err != nil || decoded != "json" {
		t.Fatalf("DecodeAs[any] = %#v, %v", decoded, err)
	}
}

func TestGenericClassEncodingRejectsCyclesWithoutResettingTraversal(t *testing.T) {
	node := &genericTestNode[int64]{Value: 1}
	node.Next = node
	input := Any(node)
	if input.err == nil || !strings.Contains(input.err.Error(), "cyclic") {
		t.Fatalf("cyclic generic class input error = %v", input.err)
	}
}

func TestMarkerInterfaceInstantiationsFailNormallyInsteadOfPanicking(t *testing.T) {
	if descriptor := TypeOf[DynamicType](); descriptor.value != nil {
		t.Fatalf("TypeOf[DynamicType] = %#v", descriptor)
	}
	value := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "value"}}}
	if _, err := DecodeAs[DynamicDecoder](value); err == nil {
		t.Fatal("DecodeAs[DynamicDecoder] unexpectedly succeeded")
	}
	if _, err := DecodeAs[DynamicEnum](value); err == nil {
		t.Fatal("DecodeAs[DynamicEnum] unexpectedly succeeded")
	}
}

func TestStructuredGenericTypeArgumentsRejectInvalidMetadata(t *testing.T) {
	tests := []struct {
		name string
		args []TypeArgument
	}{
		{"empty name", []TypeArgument{{Type: PrimitiveBAMLType(IntType)}}},
		{"duplicate", []TypeArgument{{Name: "T", Type: PrimitiveBAMLType(IntType)}, {Name: "T", Type: PrimitiveBAMLType(StringType)}}},
		{"missing descriptor", []TypeArgument{{Name: "T"}}},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if _, _, err := encodeCallForDispatchWithTypeArgs(1, nil, test.args); err == nil {
				t.Fatal("invalid type arguments were accepted")
			}
		})
	}
}

func TestGenericClassAnyPreservesTypeArgumentsAndInputMarshaler(t *testing.T) {
	input := Any(genericTestBox[int64]{Value: 42})
	encoded, err := input.encodeValue(nil)
	if err != nil {
		t.Fatal(err)
	}
	class := encoded.GetClassValue()
	classType := encoded.GetValueType().GetClassTy()
	if class == nil || classType == nil {
		t.Fatalf("class = %#v", class)
	}
	if classType.Name != "user.tests.Box" || len(classType.TypeArgs) != 1 {
		t.Fatalf("class type = %#v", classType)
	}
	if got := (BAMLType{value: classType.TypeArgs[0]}); !got.Equal(PrimitiveBAMLType(IntType)) {
		t.Fatalf("class type arg = %#v", got)
	}
	if len(class.Fields) != 1 || class.Fields[0].GetStringKey() != "value" || class.Fields[0].Value.GetIntValue() != 42 {
		t.Fatalf("fields = %#v", class.Fields)
	}
}

func TestDecodeAsReifiesNestedGenericClassesAndContainers(t *testing.T) {
	integer := PrimitiveBAMLType(IntType).value
	innerType := ClassBAMLType("user.tests.Box", PrimitiveBAMLType(IntType)).value
	inner := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{
		Name: "user.tests.Box", TypeArgs: []*cffi.BamlTy{integer}, Fields: []*cffi.BamlOutboundMapEntry{{
			Key: "value", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 7}},
		}},
	}}}
	outer := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{
		Name: "user.tests.Box", TypeArgs: []*cffi.BamlTy{innerType}, Fields: []*cffi.BamlOutboundMapEntry{{Key: "value", Value: inner}},
	}}}}
	decoded, err := DecodeAs[genericTestBox[genericTestBox[int64]]](outer)
	if err != nil {
		t.Fatal(err)
	}
	if decoded.Value.Value != 7 {
		t.Fatalf("decoded = %#v", decoded)
	}

	list := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{ListValue: &cffi.BamlValueList{
		Items: []*cffi.BamlOutboundValue{inner}, ItemType: innerType,
	}}}}
	boxes, err := DecodeAs[[]genericTestBox[int64]](list)
	if err != nil || len(boxes) != 1 || boxes[0].Value != 7 {
		t.Fatalf("list = %#v, %v", boxes, err)
	}
}

func TestDecodeAsRejectsWrongGenericClassArgument(t *testing.T) {
	value := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{
		Name: "user.tests.Box", TypeArgs: []*cffi.BamlTy{PrimitiveBAMLType(StringType).value}, Fields: []*cffi.BamlOutboundMapEntry{{
			Key: "value", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 7}},
		}},
	}}}}
	if _, err := DecodeAs[genericTestBox[int64]](value); err == nil {
		t.Fatal("wrong generic class argument was accepted")
	}
}

func TestDynamicGenericUnionPreservesNominalCandidateTypes(t *testing.T) {
	integer := PrimitiveBAMLType(IntType)
	classType := ClassBAMLType("user.tests.Box", integer)
	classPayload := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{
		Name: "user.tests.Box", TypeArgs: []*cffi.BamlTy{integer.value}, Fields: []*cffi.BamlOutboundMapEntry{{
			Key: "value", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 11}},
		}},
	}}}
	decoder := DynamicUnionDecoder(DynamicDecodeCandidate{
		Type: classType, Decode: DynamicDecodeAs[genericTestBox[int64]],
	})
	decoded, err := decoder(outboundUnion(UnionBAMLType(classType, PrimitiveBAMLType(StringType)), uint32Pointer(0), classPayload))
	box, ok := decoded.(genericTestBox[int64])
	if err != nil || !ok || box.Value != 11 {
		t.Fatalf("dynamic class = %#v, %v", decoded, err)
	}

	enumType := EnumBAMLType("user.tests.Enum")
	enumPayload := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_EnumValue{EnumValue: &cffi.BamlValueEnum{
		Name: "user.tests.Enum", Value: "ONE",
	}}}
	decoder = DynamicUnionDecoder(DynamicDecodeCandidate{
		Type: enumType, Decode: DynamicDecodeAs[genericTestEnum],
	})
	decoded, err = decoder(outboundUnion(UnionBAMLType(enumType, PrimitiveBAMLType(StringType)), uint32Pointer(0), enumPayload))
	gotEnum, ok := decoded.(genericTestEnum)
	if err != nil || !ok || gotEnum != "ONE" {
		t.Fatalf("dynamic enum = %#v, %v", decoded, err)
	}
}
