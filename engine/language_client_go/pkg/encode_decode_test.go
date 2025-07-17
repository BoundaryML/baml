package baml

import (
	"fmt"
	"math"
	"reflect"
	"testing"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/serde"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
	"github.com/ghetzel/testify/assert"
	"github.com/ghetzel/testify/require"
)

var test_type_map TypeMap

func set_test_type_map(type_map TypeMap) {
	// Set the type map for testing
	test_type_map = type_map
}

// Helper function for round-trip testing
func testRoundTrip(t *testing.T, name string, value interface{}, expected interface{}) {
	t.Helper()
	t.Run(name, func(t *testing.T) {
		encoded, err := serde.BAMLTESTINGONLY_InternalEncode(value)
		require.NoError(t, err, "encoding should not fail")

		decoded := serde.Decode(encoded, test_type_map).Interface()
		if expected != nil {
			assert.Equal(t, expected, decoded, "decoded value should match expected")
		} else {
			assert.Equal(t, value, decoded, "decoded value should match original")
		}
	})
}

func TestEncodeDecodeRoundTrip(t *testing.T) {

	t.Run("PrimitiveTypes", func(t *testing.T) {
		// Strings
		testRoundTrip(t, "String", "hello world", nil)
		testRoundTrip(t, "EmptyString", "", nil)
		testRoundTrip(t, "UnicodeString", "Hello 世界 🌍", nil)
		testRoundTrip(t, "StringWithSpecialChars", "line1\nline2\ttab\r\nwindows", nil)

		// Integers - all decode to int64
		testRoundTrip(t, "Int", 42, int64(42))
		testRoundTrip(t, "NegativeInt", -42, int64(-42))
		testRoundTrip(t, "ZeroInt", 0, int64(0))
		testRoundTrip(t, "MaxInt64", int64(math.MaxInt64), nil)
		testRoundTrip(t, "MinInt64", int64(math.MinInt64), nil)
		testRoundTrip(t, "Int8", int8(127), int64(127))
		testRoundTrip(t, "Int16", int16(32767), int64(32767))
		testRoundTrip(t, "Int32", int32(2147483647), int64(2147483647))

		// Floats - all decode to float64
		testRoundTrip(t, "Float64", 3.14159, nil)
		testRoundTrip(t, "NegativeFloat", -3.14159, nil)
		testRoundTrip(t, "ZeroFloat", 0.0, nil)
		testRoundTrip(t, "Float32", float32(3.14), float64(float32(3.14)))
		testRoundTrip(t, "VerySmallFloat", 1e-10, nil)
		testRoundTrip(t, "VeryLargeFloat", 1e10, nil)

		// Booleans
		testRoundTrip(t, "BoolTrue", true, nil)
		testRoundTrip(t, "BoolFalse", false, nil)

		// Nil
		testRoundTrip(t, "Nil", nil, (*interface{})(nil))
	})

	t.Run("PointerTypes", func(t *testing.T) {
		// Note: Individual pointers decode to their dereferenced values or nil
		// This is expected behavior since the decoder doesn't have type info
		str := "hello"
		testRoundTrip(t, "StringPointer", &str, nil)

		var nilStr *string
		testRoundTrip(t, "NilStringPointer", nilStr, nil)

		num := 42
		var num64 int64 = 42
		testRoundTrip(t, "IntPointer", &num, &num64)

		var nilInt *int
		testRoundTrip(t, "NilIntPointer", nilInt, (*int64)(nil))

		flt := 3.14
		testRoundTrip(t, "FloatPointer", &flt, nil)

		var nilFloat *float64
		testRoundTrip(t, "NilFloatPointer", nilFloat, nil)

		bl := true
		testRoundTrip(t, "BoolPointer", &bl, nil)

		var nilBool *bool
		testRoundTrip(t, "NilBoolPointer", nilBool, nil)
	})

	t.Run("SliceTypes", func(t *testing.T) {
		// String slices
		testRoundTrip(t, "StringSlice", []string{"a", "b", "c"}, nil)
		testRoundTrip(t, "EmptyStringSlice", []string{}, nil)
		testRoundTrip(t, "SingleElementSlice", []string{"one"}, nil)

		// Int slices - decode to []int64
		testRoundTrip(t, "IntSlice", []int{1, 2, 3, 4, 5}, []int64{1, 2, 3, 4, 5})
		testRoundTrip(t, "EmptyIntSlice", []int{}, []int64{})
		testRoundTrip(t, "MixedIntSlice", []int{-1, 0, 1}, []int64{-1, 0, 1})

		// Float slices
		testRoundTrip(t, "FloatSlice", []float64{1.1, 2.2, 3.3}, nil)
		testRoundTrip(t, "EmptyFloatSlice", []float64{}, nil)

		// Bool slices
		testRoundTrip(t, "BoolSlice", []bool{true, false, true}, nil)
		testRoundTrip(t, "EmptyBoolSlice", []bool{}, nil)

		// Skip slices of pointers for now - they have complex type issues
		// The decoder can't properly reconstruct slice element types without schema
	})

	t.Run("OptionalSliceTypes", func(t *testing.T) {
		// Optional slices (pointer to slice)
		stringSlice := []string{"a", "b", "c"}
		testRoundTrip(t, "OptionalStringSlice", &stringSlice, nil)

		var nilStringSlice *[]string
		testRoundTrip(t, "NilOptionalStringSlice", nilStringSlice, nil)

		intSlice := []int{1, 2, 3}
		expectedIntSlice := []int64{1, 2, 3}
		testRoundTrip(t, "OptionalIntSlice", &intSlice, &expectedIntSlice)

		var nilIntSlice *[]int
		var expectedNilIntSlice *[]int64
		testRoundTrip(t, "NilOptionalIntSlice", nilIntSlice, expectedNilIntSlice)

		floatSlice := []float64{1.1, 2.2, 3.3}
		testRoundTrip(t, "OptionalFloatSlice", &floatSlice, nil)

		var nilFloatSlice *[]float64
		testRoundTrip(t, "NilOptionalFloatSlice", nilFloatSlice, nil)

		boolSlice := []bool{true, false, true}
		testRoundTrip(t, "OptionalBoolSlice", &boolSlice, nil)

		var nilBoolSlice *[]bool
		testRoundTrip(t, "NilOptionalBoolSlice", nilBoolSlice, nil)

		// Empty optional slices
		emptyStringSlice := []string{}
		testRoundTrip(t, "EmptyOptionalStringSlice", &emptyStringSlice, nil)

		emptyIntSlice := []int{}
		expectedEmptyIntSlice := []int64{}
		testRoundTrip(t, "EmptyOptionalIntSlice", &emptyIntSlice, &expectedEmptyIntSlice)
	})

	t.Run("MapTypes", func(t *testing.T) {
		// Basic maps
		testRoundTrip(t, "StringMap", map[string]string{
			"key1": "value1",
			"key2": "value2",
		}, nil)
		testRoundTrip(t, "EmptyMap", map[string]string{}, nil)
		testRoundTrip(t, "SingleElementMap", map[string]string{"only": "one"}, nil)

		testRoundTrip(t, "IntMap", map[string]int{
			"one":   1,
			"two":   2,
			"three": 3,
		}, map[string]int64{
			"one":   1,
			"two":   2,
			"three": 3,
		})

		testRoundTrip(t, "FloatMap", map[string]float64{
			"pi": 3.14159,
			"e":  2.71828,
		}, nil)

		testRoundTrip(t, "BoolMap", map[string]bool{
			"yes": true,
			"no":  false,
		}, nil)

		// Maps with pointer values - these should maintain pointer semantics
		v1, v2, v3 := "val1", "val2", "val3"
		testRoundTrip(t, "StringPointerMap", map[string]*string{
			"a": &v1,
			"b": &v2,
			"c": &v3,
		}, nil)

		testRoundTrip(t, "StringPointerMapWithNils", map[string]*string{
			"a": &v1,
			"b": nil,
			"c": &v3,
			"d": nil,
		}, nil)

		testRoundTrip(t, "AllNilStringPointerMap", map[string]*string{
			"a": nil,
			"b": nil,
			"c": nil,
		}, nil)

		n1, n2 := 100, 200
		n1_64, n2_64 := int64(100), int64(200)
		testRoundTrip(t, "IntPointerMap", map[string]*int{
			"first":  &n1,
			"second": nil,
			"third":  &n2,
		}, map[string]*int64{
			"first":  &n1_64,
			"second": nil,
			"third":  &n2_64,
		})

		f1, f2 := 1.5, 2.5
		testRoundTrip(t, "FloatPointerMap", map[string]*float64{
			"x": &f1,
			"y": nil,
			"z": &f2,
		}, nil)

		b1, b2 := true, false
		testRoundTrip(t, "BoolPointerMap", map[string]*bool{
			"enabled":  &b1,
			"disabled": &b2,
			"unknown":  nil,
		}, nil)
	})

	t.Run("OptionalMapTypes", func(t *testing.T) {
		// Optional maps (pointer to map)
		stringMap := map[string]string{
			"key1": "value1",
			"key2": "value2",
		}
		testRoundTrip(t, "OptionalStringMap", &stringMap, nil)

		var nilStringMap *map[string]string
		testRoundTrip(t, "NilOptionalStringMap", nilStringMap, nil)

		intMap := map[string]int{
			"one": 1,
			"two": 2,
		}
		expectedIntMap := map[string]int64{
			"one": 1,
			"two": 2,
		}
		testRoundTrip(t, "OptionalIntMap", &intMap, &expectedIntMap)

		var nilIntMap *map[string]int
		var expectedNilIntMap *map[string]int64
		testRoundTrip(t, "NilOptionalIntMap", nilIntMap, expectedNilIntMap)

		floatMap := map[string]float64{
			"pi": 3.14159,
			"e":  2.71828,
		}
		testRoundTrip(t, "OptionalFloatMap", &floatMap, nil)

		var nilFloatMap *map[string]float64
		testRoundTrip(t, "NilOptionalFloatMap", nilFloatMap, nil)

		boolMap := map[string]bool{
			"yes": true,
			"no":  false,
		}
		testRoundTrip(t, "OptionalBoolMap", &boolMap, nil)

		var nilBoolMap *map[string]bool
		testRoundTrip(t, "NilOptionalBoolMap", nilBoolMap, nil)

		// Empty optional maps
		emptyStringMap := map[string]string{}
		testRoundTrip(t, "EmptyOptionalStringMap", &emptyStringMap, nil)

		emptyIntMap := map[string]int{}
		expectedEmptyIntMap := map[string]int64{}
		testRoundTrip(t, "EmptyOptionalIntMap", &emptyIntMap, &expectedEmptyIntMap)

		// Optional maps with pointer values (maps of optionals)
		v1, v2 := "val1", "val2"
		stringPointerMap := map[string]*string{
			"a": &v1,
			"b": nil,
			"c": &v2,
		}
		testRoundTrip(t, "OptionalStringPointerMap", &stringPointerMap, nil)

		var nilStringPointerMap *map[string]*string
		testRoundTrip(t, "NilOptionalStringPointerMap", nilStringPointerMap, nil)

		n1, n2 := 100, 200
		n1_64, n2_64 := int64(100), int64(200)
		intPointerMap := map[string]*int{
			"first":  &n1,
			"second": nil,
			"third":  &n2,
		}
		expectedIntPointerMap := map[string]*int64{
			"first":  &n1_64,
			"second": nil,
			"third":  &n2_64,
		}
		testRoundTrip(t, "OptionalIntPointerMap", &intPointerMap, &expectedIntPointerMap)
	})

	t.Run("NestedStructures", func(t *testing.T) {
		// Map of slices
		testRoundTrip(t, "MapOfStringSlices", map[string][]string{
			"fruits":     {"apple", "banana", "orange"},
			"vegetables": {"carrot", "lettuce"},
			"empty":      {},
		}, nil)

		testRoundTrip(t, "MapOfIntSlices", map[string][]int{
			"evens":  {2, 4, 6, 8},
			"odds":   {1, 3, 5, 7},
			"primes": {2, 3, 5, 7, 11},
		}, map[string][]int64{
			"evens":  {2, 4, 6, 8},
			"odds":   {1, 3, 5, 7},
			"primes": {2, 3, 5, 7, 11},
		})

		// Slice of maps
		testRoundTrip(t, "SliceOfMaps", []map[string]string{
			{"name": "Alice", "role": "admin"},
			{"name": "Bob", "role": "user"},
			{},
		}, nil)

		// Map of maps
		testRoundTrip(t, "MapOfMaps", map[string]map[string]int{
			"scores": {"alice": 100, "bob": 85},
			"ages":   {"alice": 30, "bob": 25},
			"empty":  {},
		}, map[string]map[string]int64{
			"scores": {"alice": 100, "bob": 85},
			"ages":   {"alice": 30, "bob": 25},
			"empty":  {},
		})
	})

	t.Run("EdgeCases", func(t *testing.T) {
		// Very large collections
		largeSlice := make([]int64, 1000)
		for i := range largeSlice {
			largeSlice[i] = int64(i)
		}
		testRoundTrip(t, "LargeSlice", largeSlice, nil)

		largeMap := make(map[string]int64)
		for i := 0; i < 1000; i++ {
			key := string(rune('a'+i%26)) + "_" + string(rune('0'+i%10))
			largeMap[key] = int64(i)
		}
		testRoundTrip(t, "LargeMap", largeMap, nil)

		// Unicode keys in maps
		testRoundTrip(t, "UnicodeMapKeys", map[string]string{
			"Hello": "World",
			"你好":    "世界",
			"🔑":     "🌍",
			"مرحبا": "عالم",
		}, nil)

		// Special characters in values
		testRoundTrip(t, "SpecialCharValues", map[string]string{
			"newline":   "line1\nline2",
			"tab":       "col1\tcol2",
			"quote":     `"quoted"`,
			"backslash": `path\to\file`,
			"null_char": "before\x00after",
		}, nil)

		// Skip mixed nil complex structure test since interface{} is not supported
	})

	t.Run("ArrayTypes", func(t *testing.T) {
		// Fixed size arrays - decode as slices
		testRoundTrip(t, "StringArray", [3]string{"a", "b", "c"}, []string{"a", "b", "c"})
		testRoundTrip(t, "IntArray", [5]int{1, 2, 3, 4, 5}, []int64{1, 2, 3, 4, 5})
		testRoundTrip(t, "FloatArray", [2]float64{1.1, 2.2}, []float64{1.1, 2.2})
		testRoundTrip(t, "BoolArray", [4]bool{true, false, true, false}, []bool{true, false, true, false})

		// Skip arrays with pointers - same issues as slices of pointers
	})
}

// Test structures implementing BAML interfaces

// TestClass implements BamlSerializer
type TestClass struct {
	Name string
	Age  int64
	Tags []string
}

func (t TestClass) BamlTypeName() string {
	return "TestClass"
}

func (t TestClass) BamlEncodeName() *cffi.CFFITypeName {
	return &cffi.CFFITypeName{
		Name:      t.BamlTypeName(),
		Namespace: cffi.CFFITypeNamespace_TYPES,
	}
}

func (t TestClass) Encode() (*cffi.CFFIValueHolder, error) {
	fields := map[string]any{
		"name": t.Name,
		"age":  t.Age,
		"tags": t.Tags,
	}
	return serde.EncodeClass(t.BamlEncodeName, fields, nil)
}

func (t *TestClass) Decode(holder *cffi.CFFIValueClass, typeMap TypeMap) {
	typeName := holder.Name
	if typeName.Namespace != cffi.CFFITypeNamespace_TYPES {
		panic(fmt.Sprintf("expected cffi.CFFITypeNamespace_TYPES, got %s", string(typeName.Namespace.String())))
	}
	if typeName.Name != "TestClass" {
		panic(fmt.Sprintf("expected TestClass, got %s", typeName.Name))
	}

	for _, field := range holder.Fields {
		value := serde.Decode(field.Value, typeMap).Interface()
		switch field.Key {
		case "name":
			t.Name = value.(string)
		case "age":
			t.Age = value.(int64)
		case "tags":
			t.Tags = value.([]string)
		default:
			panic(fmt.Sprintf("unexpected property '%s' in class TestClass", field.Key))
		}
	}
}

// TestEnum implements BamlSerializer
type TestEnum string

const (
	TestEnumValue1 TestEnum = "VALUE1"
	TestEnumValue2 TestEnum = "VALUE2"
	TestEnumValue3 TestEnum = "VALUE3"
)

func (e TestEnum) BamlTypeName() string {
	return "TestEnum"
}

func (e TestEnum) BamlEncodeName() *cffi.CFFITypeName {
	return &cffi.CFFITypeName{
		Name:      e.BamlTypeName(),
		Namespace: cffi.CFFITypeNamespace_TYPES,
	}
}

func (e TestEnum) Encode() (*cffi.CFFIValueHolder, error) {
	return serde.EncodeEnum(e.BamlEncodeName, string(e), false)
}

func (e *TestEnum) Decode(holder *cffi.CFFIValueEnum, typeMap TypeMap) {
	typeName := holder.Name
	if typeName.Namespace != cffi.CFFITypeNamespace_TYPES {
		panic(fmt.Sprintf("expected cffi.CFFITypeNamespace_TYPES, got %s", typeName.Namespace.String()))
	}
	if typeName.Name != "TestEnum" {
		panic(fmt.Sprintf("expected TestEnum, got %s", typeName.Name))
	}

	*e = TestEnum(holder.Value)
}

// TestUnion implements BamlSerializer
type TestUnion struct {
	VariantName string
	Value       any
}

func (u TestUnion) BamlTypeName() string {
	return "TestUnion"
}

func (u TestUnion) BamlEncodeName() *cffi.CFFITypeName {
	return &cffi.CFFITypeName{
		Name:      u.BamlTypeName(),
		Namespace: cffi.CFFITypeNamespace_TYPES,
	}
}

func (u TestUnion) Encode() (*cffi.CFFIValueHolder, error) {
	return serde.EncodeUnion(u.BamlEncodeName, u.VariantName, u.Value)
}

func (u *TestUnion) Decode(holder *cffi.CFFIValueUnionVariant, typeMap TypeMap) {
	typeName := holder.Name
	if typeName.Namespace != cffi.CFFITypeNamespace_TYPES {
		panic(fmt.Sprintf("expected cffi.CFFITypeNamespace_TYPES, got %s", typeName.Namespace.String()))
	}
	if typeName.Name != "TestUnion" {
		panic(fmt.Sprintf("expected TestUnion, got %s", typeName.Name))
	}

	u.VariantName = holder.VariantName
	u.Value = serde.Decode(holder.Value, typeMap).Interface()
}

// TestClassDeserializer implements BamlClassDeserializer
type TestClassDeserializer struct {
	Name string
	Age  int64 // Will be int64 after decoding
	Tags []string
}

func (t *TestClassDeserializer) Decode(holder *cffi.CFFIValueClass, typeMap TypeMap) {
	for _, field := range holder.Fields {
		switch field.Key {
		case "name":
			t.Name = serde.Decode(field.Value, typeMap).Interface().(string)
		case "age":
			t.Age = serde.Decode(field.Value, typeMap).Interface().(int64)
		case "tags":
			t.Tags = serde.Decode(field.Value, typeMap).Interface().([]string)
		default:
			panic(fmt.Sprintf("unknown field: %s", field.Key))
		}
	}
}

func TestCustomStructs(t *testing.T) {
	type_map := TypeMap{
		"TYPES.TestClass": reflect.TypeOf(TestClass{}),
		"TYPES.TestEnum":  reflect.TypeOf(TestEnum("")),
		"TYPES.TestUnion": reflect.TypeOf(TestUnion{}),
	}
	// set type_map for all tests here
	set_test_type_map(type_map)

	t.Cleanup(func() {
		// Reset type map after tests
		set_test_type_map(nil)
	})

	t.Run("CustomClass", func(t *testing.T) {
		// Test custom class round-trip
		testClass := TestClass{
			Name: "Alice",
			Age:  30,
			Tags: []string{"developer", "golang", "baml"},
		}

		testRoundTrip(t, "CustomClass", testClass, nil)
	})

	t.Run("CustomEnum", func(t *testing.T) {
		// Test custom enum round-trip
		testEnum := TestEnumValue2
		testRoundTrip(t, "CustomEnum", testEnum, nil)
	})

	t.Run("CustomUnion", func(t *testing.T) {
		// Test custom union round-trip with string variant
		testUnion1 := TestUnion{
			VariantName: "StringVariant",
			Value:       "hello world",
		}
		testRoundTrip(t, "CustomUnionString", testUnion1, nil)

		// Test custom union round-trip with int variant (value becomes int64)
		testUnion2 := TestUnion{
			VariantName: "IntVariant",
			Value:       42,
		}
		expectedUnion2 := TestUnion{
			VariantName: "IntVariant",
			Value:       int64(42),
		}
		testRoundTrip(t, "CustomUnionInt", testUnion2, expectedUnion2)
	})

	t.Run("OptionalCustomClass", func(t *testing.T) {
		// Test optional custom class (pointer to custom struct)
		testClass := TestClass{
			Name: "Bob",
			Age:  25,
			Tags: []string{"tester"},
		}
		// Pointers to custom types should come back as pointer
		testRoundTrip(t, "OptionalCustomClass", &testClass, nil)

		// Test nil optional custom class
		var nilClass *TestClass
		testRoundTrip(t, "NilOptionalCustomClass", nilClass, nil)

		// Test optional class with empty fields
		emptyClass := TestClass{
			Name: "",
			Age:  0,
			Tags: []string{},
		}
		testRoundTrip(t, "OptionalEmptyCustomClass", &emptyClass, nil)

		// Test optional class with nil slice field
		classWithNilSlice := TestClass{
			Name: "HasNilSlice",
			Age:  42,
			Tags: nil, // nil slice
		}
		expectedWithEmptySlice := TestClass{
			Name: "HasNilSlice",
			Age:  42,
			Tags: []string{}, // nil slices decode as empty slices
		}
		testRoundTrip(t, "OptionalClassWithNilSlice", &classWithNilSlice, &expectedWithEmptySlice)
	})

	t.Run("OptionalCustomEnum", func(t *testing.T) {
		// Test optional enum (pointer to enum)
		enum1 := TestEnumValue1
		testRoundTrip(t, "OptionalEnum1", &enum1, nil)

		enum2 := TestEnumValue2
		testRoundTrip(t, "OptionalEnum2", &enum2, nil)

		enum3 := TestEnumValue3
		testRoundTrip(t, "OptionalEnum3", &enum3, nil)

		// Test nil optional enum
		var nilEnum *TestEnum
		testRoundTrip(t, "NilOptionalEnum", nilEnum, nil)
	})

	t.Run("OptionalCustomUnion", func(t *testing.T) {
		// Test optional union with string variant
		union1 := TestUnion{
			VariantName: "StringVariant",
			Value:       "optional string",
		}
		testRoundTrip(t, "OptionalUnionString", &union1, nil)

		// Test optional union with int variant
		union2 := TestUnion{
			VariantName: "IntVariant",
			Value:       99,
		}
		expectedUnion2 := TestUnion{
			VariantName: "IntVariant",
			Value:       int64(99),
		}
		testRoundTrip(t, "OptionalUnionInt", &union2, &expectedUnion2)

		// Test optional union with bool variant
		union3 := TestUnion{
			VariantName: "BoolVariant",
			Value:       true,
		}
		testRoundTrip(t, "OptionalUnionBool", &union3, nil)

		// Test optional union with nested custom class
		nestedClass := TestClass{
			Name: "Nested",
			Age:  10,
			Tags: []string{"nested", "in", "union"},
		}
		union4 := TestUnion{
			VariantName: "ClassVariant",
			Value:       nestedClass,
		}
		testRoundTrip(t, "OptionalUnionWithClass", &union4, nil)

		// Test nil optional union
		var nilUnion *TestUnion
		testRoundTrip(t, "NilOptionalUnion", nilUnion, nil)
	})

	t.Run("NestedOptionalStructs", func(t *testing.T) {
		// Test class containing optional fields in a map structure
		// Since we can't modify TestClass, we'll test maps with optional custom types

		// Map with optional classes
		class1 := TestClass{Name: "First", Age: 1, Tags: []string{"one"}}
		class2 := TestClass{Name: "Second", Age: 2, Tags: []string{"two"}}
		classMap := map[string]*TestClass{
			"first":  &class1,
			"second": &class2,
			"nil":    nil,
		}
		expectedClassMap := map[string]*TestClass{
			"first":  &class1,
			"second": &class2,
			"nil":    nil,
		}
		testRoundTrip(t, "MapWithOptionalClasses", classMap, expectedClassMap)

		// Map with optional enums
		enum1 := TestEnumValue1
		enum2 := TestEnumValue2
		enumMap := map[string]*TestEnum{
			"val1": &enum1,
			"val2": &enum2,
			"nil":  nil,
		}
		expectedEnumMap := map[string]*TestEnum{
			"val1": &enum1,
			"val2": &enum2,
			"nil":  nil,
		}
		testRoundTrip(t, "MapWithOptionalEnums", enumMap, expectedEnumMap)

		// Map with optional unions
		union1 := TestUnion{VariantName: "Str", Value: "hello"}
		union2 := TestUnion{VariantName: "Int", Value: 42}
		expectedUnion2 := TestUnion{VariantName: "Int", Value: int64(42)}
		unionMap := map[string]*TestUnion{
			"str_union": &union1,
			"int_union": &union2,
			"nil_union": nil,
		}
		expectedUnionMap := map[string]*TestUnion{
			"str_union": &union1,
			"int_union": &expectedUnion2,
			"nil_union": nil,
		}
		testRoundTrip(t, "MapWithOptionalUnions", unionMap, expectedUnionMap)

		// Slice with optional classes
		classSlice := []*TestClass{&class1, nil, &class2}
		expectedClassSlice := []*TestClass{&class1, nil, &class2}
		testRoundTrip(t, "SliceWithOptionalClasses", classSlice, expectedClassSlice)

		// Slice with optional enums
		enumSlice := []*TestEnum{&enum1, nil, &enum2}
		expectedEnumSlice := []*TestEnum{&enum1, nil, &enum2}
		testRoundTrip(t, "SliceWithOptionalEnums", enumSlice, expectedEnumSlice)

		// Slice with optional unions
		unionSlice := []*TestUnion{&union1, nil, &union2}
		expectedUnionSlice := []*TestUnion{&union1, nil, &expectedUnion2}
		testRoundTrip(t, "SliceWithOptionalUnions", unionSlice, expectedUnionSlice)
	})

	t.Run("ComplexOptionalNesting", func(t *testing.T) {
		// Test deeply nested optional structures

		// Optional slice of optional classes
		class1 := TestClass{Name: "Deep1", Age: 100, Tags: []string{"deep"}}
		class2 := TestClass{Name: "Deep2", Age: 200, Tags: []string{"deeper"}}
		optionalSliceOfOptionalClasses := &[]*TestClass{&class1, nil, &class2}
		testRoundTrip(t, "OptionalSliceOfOptionalClasses", optionalSliceOfOptionalClasses, nil)

		// Optional map of optional enums
		enum1 := TestEnumValue1
		enum3 := TestEnumValue3
		optionalMapOfOptionalEnums := &map[string]*TestEnum{
			"first": &enum1,
			"nil":   nil,
			"third": &enum3,
		}
		testRoundTrip(t, "OptionalMapOfOptionalEnums", optionalMapOfOptionalEnums, nil)

		// Union containing optional class
		optionalClass := &TestClass{Name: "InUnion", Age: 333, Tags: []string{"union", "optional"}}
		unionWithOptionalClass := TestUnion{
			VariantName: "OptionalClassVariant",
			Value:       optionalClass,
		}
		testRoundTrip(t, "UnionWithOptionalClass", unionWithOptionalClass, nil)

		// Union containing nil optional class
		var nilOptionalClass *TestClass
		unionWithNilOptionalClass := TestUnion{
			VariantName: "NilOptionalClassVariant",
			Value:       nilOptionalClass,
		}
		testRoundTrip(t, "UnionWithNilOptionalClass", unionWithNilOptionalClass, nil)
	})

	t.Run("CustomClassWithDynamicFields", func(t *testing.T) {
		// Test encoding a class with dynamic fields using EncodeClass directly
		staticFields := map[string]any{}

		dynamicFields := map[string]any{
			"id":     123,
			"name":   "Static Field Test",
			"extra1": "dynamic value 1",
			"extra2": 456,
			"extra3": []string{"a", "b", "c"},
		}

		nameEncoder := func() *cffi.CFFITypeName {
			return &cffi.CFFITypeName{Name: "DynamicTestClass", Namespace: cffi.CFFITypeNamespace_TYPES}
		}

		encoded, err := serde.EncodeClass(nameEncoder, staticFields, &dynamicFields)
		require.NoError(t, err, "encoding class with dynamic fields should not fail")

		assert.NotNil(t, encoded)
		classValue, ok := encoded.Value.(*cffi.CFFIValueHolder_ClassValue)
		require.True(t, ok, "encoded value should be a class")

		assert.Equal(t, "DynamicTestClass", classValue.ClassValue.Name.Name)
		assert.Len(t, classValue.ClassValue.Fields, 0)        // static fields
		assert.Len(t, classValue.ClassValue.DynamicFields, 5) // dynamic fields

		// Test decoding with DynamicClass
		decoded := &serde.DynamicClass{}
		decoded.Decode(classValue.ClassValue, type_map)

		assert.Equal(t, "DynamicTestClass", decoded.Name)

		// Check static fields
		assert.Equal(t, int64(123), decoded.Fields["id"])
		assert.Equal(t, "Static Field Test", decoded.Fields["name"])

		// Check dynamic fields
		assert.Equal(t, "dynamic value 1", decoded.Fields["extra1"])
		assert.Equal(t, int64(456), decoded.Fields["extra2"])
		assert.Equal(t, []string{"a", "b", "c"}, decoded.Fields["extra3"])
	})

	t.Run("NestedCustomStructs", func(t *testing.T) {
		// Test nested custom structures - union containing a custom class
		innerClass := TestClass{
			Name: "Inner",
			Age:  20,
			Tags: []string{"inner"},
		}

		unionWithClass := TestUnion{
			VariantName: "ClassVariant",
			Value:       innerClass,
		}

		testRoundTrip(t, "NestedCustomStructs", unionWithClass, nil)
	})
}

func TestEncodeDecodeErrors(t *testing.T) {
	t.Run("UnsupportedTypes", func(t *testing.T) {
		// Test encoding unsupported types
		type CustomStruct struct {
			Field string
		}

		_, err := serde.BAMLTESTINGONLY_InternalEncode(CustomStruct{Field: "test"})
		assert.Error(t, err, "should error on unsupported struct type")

		_, err = serde.BAMLTESTINGONLY_InternalEncode(make(chan int))
		assert.Error(t, err, "should error on channel type")

		_, err = serde.BAMLTESTINGONLY_InternalEncode(func() {})
		assert.Error(t, err, "should error on function type")
	})

	t.Run("NonStringMapKeys", func(t *testing.T) {
		// Maps with non-string keys should fail
		_, err := serde.BAMLTESTINGONLY_InternalEncode(map[int]string{1: "one", 2: "two"})
		assert.Error(t, err, "should error on non-string map keys")
	})
}
