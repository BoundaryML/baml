package main

import (
	"context"
	"testing"

	b "example.com/integ-tests/baml_client"
	"example.com/integ-tests/baml_client/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestClassWithLiteralProp tests class with literal properties
// Reference: test_functions.py:186-188
func TestClassWithLiteralProp(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.FnLiteralClassInputOutput(ctx, types.LiteralClassHello{
		Prop: "hello",
	})
	
	require.NoError(t, err)
	assert.IsType(t, types.LiteralClassHello{}, result)
}

// TestLiteralClassWithLiteralUnionProp tests literal class with union property
// Reference: test_functions.py:191-193
func TestLiteralClassWithLiteralUnionProp(t *testing.T) {
	ctx := context.Background()
	
	input := types.Union2LiteralClassOneOrLiteralClassTwo__NewLiteralClassOne(types.LiteralClassOne{
		Prop: "one",
	})
	
	result, err := b.FnLiteralUnionClassInputOutput(ctx, input)
	require.NoError(t, err)
	assert.NotNil(t, result)
}

// TestSingleMapStringToString tests map[string]string
// Reference: test_functions.py:196-200
func TestSingleMapStringToString(t *testing.T) {
	ctx := context.Background()
	
	inputMap := map[string]string{
		"lorem": "ipsum",
		"dolor": "sit",
	}
	
	result, err := b.TestFnNamedArgsSingleMapStringToString(ctx, inputMap)
	require.NoError(t, err)
	assert.Contains(t, result, "lorem")
}

// TestSingleMapStringToClass tests map[string]Class
// Reference: test_functions.py:203-207
func TestSingleMapStringToClass(t *testing.T) {
	ctx := context.Background()
	
	inputMap := map[string]types.StringToClassEntry{
		"lorem": {Word: "ipsum"},
	}
	
	result, err := b.TestFnNamedArgsSingleMapStringToClass(ctx, inputMap)
	require.NoError(t, err)
	assert.Equal(t, "ipsum", result["lorem"].Word)
}

// TestSingleMapStringToMap tests map[string]map[string]string
// Reference: test_functions.py:210-212
func TestSingleMapStringToMap(t *testing.T) {
	ctx := context.Background()
	
	inputMap := map[string]map[string]string{
		"lorem": {"word": "ipsum"},
	}
	
	result, err := b.TestFnNamedArgsSingleMapStringToMap(ctx, inputMap)
	require.NoError(t, err)
	assert.Equal(t, "ipsum", result["lorem"]["word"])
}

// TODO: Fix this test
// TestEnumKeyInMap tests enum keys in maps
// Reference: test_functions.py:215-218
// func TestEnumKeyInMap(t *testing.T) {
// 	ctx := context.Background()
	
// 	input1 := map[types.MapKey]string{
// 		types.MapKeyA: "A",
// 	}
// 	input2 := map[types.MapKey]string{
// 		types.MapKeyB: "B", 
// 	}
	
// 	result, err := b.InOutEnumMapKey(ctx, input1, input2)
// 	require.NoError(t, err)
// 	assert.Equal(t, "A", result[types.MapKeyA])
// 	assert.Equal(t, "B", result[types.MapKeyB])
// }

// TODO: Fix this test
// TestLiteralStringUnionKeyInMap tests literal string union keys
// Reference: test_functions.py:221-224
// func TestLiteralStringUnionKeyInMap(t *testing.T) {
// 	ctx := context.Background()
	
// 	input1 := map[types.Union4KfourOrKoneOrKthreeOrKtwo]string{
// 		types.Union4KfourOrKoneOrKthreeOrKtwo__NewKone(): "1",
// 	}
// 	input2 := map[types.Union4KfourOrKoneOrKthreeOrKtwo]string{
// 		types.Union4KfourOrKoneOrKthreeOrKtwo__NewKtwo(): "2",
// 	}
	
// 	result, err := b.InOutLiteralStringUnionMapKey(ctx, input1, input2)
// 	require.NoError(t, err)
	
// 	// Verify we get the expected mappings back
// 	assert.Contains(t, result, types.Union4KfourOrKoneOrKthreeOrKtwo__NewKone())
// 	assert.Contains(t, result, types.Union4KfourOrKoneOrKthreeOrKtwo__NewKtwo())
// }

// TestSingleLiteralStringKeyInMap tests single literal string key
// Reference: test_functions.py:227-229
func TestSingleLiteralStringKeyInMap(t *testing.T) {
	ctx := context.Background()
	
	inputMap := map[string]string{
		"key": "1",
	}
	
	result, err := b.InOutSingleLiteralStringMapKey(ctx, inputMap)
	require.NoError(t, err)
	assert.Equal(t, "1", result["key"])
}

// TestPrimitiveUnionAlias tests primitive union aliases
// Reference: test_functions.py:232-234
func TestPrimitiveUnionAlias(t *testing.T) {
	ctx := context.Background()
	
	input := types.Union4BoolOrFloatOrIntOrString__NewString("test")
	
	result, err := b.PrimitiveAlias(ctx, input)
	require.NoError(t, err)
	assert.Equal(t, input, result)
}

// TestMapAlias tests map type aliases
// Reference: test_functions.py:237-239
func TestMapAlias(t *testing.T) {
	ctx := context.Background()
	
	inputMap := map[string][]string{
		"A": {"B", "C"},
		"B": {},
		"C": {},
	}
	
	result, err := b.MapAlias(ctx, inputMap)
	require.NoError(t, err)
	assert.Equal(t, inputMap, result)
}

// TestNestedAlias tests nested aliases
// Reference: test_functions.py:242-247
func TestNestedAlias(t *testing.T) {
	ctx := context.Background()
	
	// Test with string
	stringInput := types.Union6BoolOrFloatOrIntOrListStringOrMapStringKeyListStringValueOrString__NewString("test")
	stringResult, err := b.NestedAlias(ctx, stringInput)
	require.NoError(t, err)
	assert.Equal(t, stringInput, stringResult)
	
	// Test with map
	mapInput := types.Union6BoolOrFloatOrIntOrListStringOrMapStringKeyListStringValueOrString__NewMapStringKeyListStringValue(map[string][]string{
		"A": {"B", "C"},
		"B": {},
		"C": {},
	})
	mapResult, err := b.NestedAlias(ctx, mapInput)
	require.NoError(t, err)
	assert.Equal(t, mapInput, mapResult)
}

// TestOptionalListAndMap tests optional lists and maps
// Reference: test_functions.py:90-95
func TestOptionalListAndMap(t *testing.T) {
	ctx := context.Background()
	
	// Test with nil values
	result, err := b.AllowedOptionals(ctx, types.OptionalListAndMap{
		P: nil,
		Q: nil,
	})
	require.NoError(t, err)
	assert.Nil(t, result.P)
	assert.Nil(t, result.Q)
	
	// Test with values
	testList := []string{"example1"}
	testMap := map[string]string{"example2": "ok"}
	result, err = b.AllowedOptionals(ctx, types.OptionalListAndMap{
		P: &testList,
		Q: &testMap,
	})
	require.NoError(t, err)
	assert.NotNil(t, result.P)
	assert.NotNil(t, result.Q)
	assert.Equal(t, testList, *result.P)
	assert.Equal(t, testMap, *result.Q)
}

// TestReturnLiteralUnion tests literal union returns
// Reference: test_functions.py:85-87
func TestReturnLiteralUnion(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.LiteralUnionsTest(ctx, "a")
	require.NoError(t, err)

	
	// Should return one of: 1, true, or "string output"
	// Check if it matches any of the expected values
	if result.AsIntK1() != nil {
		assert.Equal(t, int64(1), *result.AsIntK1())
	} else if result.AsBoolKTrue() != nil {
		assert.True(t, *result.AsBoolKTrue()) 
	} else if result.AsKstring_output() != nil {
		assert.Equal(t, "string output", *result.AsKstring_output())
	} else {
		t.Fatalf("Result should match one of the expected union variants")
	}
}

// TestJsonTypeAliasCycle tests recursive JSON type aliases
// Reference: test_functions.py:308-331
func TestJsonTypeAliasCycle(t *testing.T) {
	ctx := context.Background()
	
	// Create complex nested JSON structure
	// Create JsonObject (map[string]JsonValue)
	nestedObj := types.JsonObject{
		"number": types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewInt(1),
		"string": types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewString("test"),
		"bool":   types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewBool(true),
		"list":   types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewJsonArray(
			[]types.JsonValue{
				types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewInt(1),
				types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewInt(2),
				types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewInt(3),
			},
		),
	}
	
	testData := types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewJsonObject(
		types.JsonObject{
			"number": types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewInt(1),
			"string": types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewString("test"),
			"bool":   types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewBool(true),
			"list":   types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewJsonArray(
				[]types.JsonValue{
					types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewInt(1),
					types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewInt(2),
					types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewInt(3),
				},
			),
			"object": types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewJsonObject(nestedObj),
			"json": types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewJsonObject(
				types.JsonObject{
					"number": types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewInt(1),
					"string": types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewString("test"),
					"bool":   types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewBool(true),
					"list":   types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewJsonArray(
						[]types.JsonValue{
							types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewInt(1),
							types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewInt(2),
							types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewInt(3),
						},
					),
					"object": types.Union6BoolOrFloatOrIntOrJsonArrayOrJsonObjectOrString__NewJsonObject(nestedObj),
				},
			),
		},
	)
	
	result, err := b.JsonTypeAliasCycle(ctx, testData)
	require.NoError(t, err)
	
	// Basic structure checks - result is a JsonValue union
	assert.True(t, result.IsJsonObject(), "Expected result to be JsonObject")
	
	if result.AsJsonObject() != nil {
		resultObj := *result.AsJsonObject()
		
		// Check number field
		if numberVal, exists := resultObj["number"]; exists {
			assert.True(t, numberVal.IsInt(), "Expected number to be int")
			if numberVal.AsInt() != nil {
				assert.Equal(t, int64(1), *numberVal.AsInt())
			}
		}
		
		// Check string field
		if stringVal, exists := resultObj["string"]; exists {
			assert.True(t, stringVal.IsString(), "Expected string to be string")
			if stringVal.AsString() != nil {
				assert.Equal(t, "test", *stringVal.AsString())
			}
		}
		
		// Check bool field
		if boolVal, exists := resultObj["bool"]; exists {
			assert.True(t, boolVal.IsBool(), "Expected bool to be bool")
			if boolVal.AsBool() != nil {
				assert.Equal(t, true, *boolVal.AsBool())
			}
		}
	}
	
	// Additional verification that the JSON cycle worked
	if result.AsJsonObject() != nil {
		resultObj := *result.AsJsonObject()
		
		// Check nested list if it exists
		if listVal, exists := resultObj["list"]; exists && listVal.IsJsonArray() {
			if listVal.AsJsonArray() != nil {
				array := *listVal.AsJsonArray()
				assert.Len(t, array, 3, "Expected array to have 3 elements")
				// Check first element
				if len(array) > 0 && array[0].IsInt() && array[0].AsInt() != nil {
					assert.Equal(t, int64(1), *array[0].AsInt())
				}
			}
		}
		
		// Check deeply nested structure
		if jsonField, exists := resultObj["json"]; exists && jsonField.IsJsonObject() {
			if jsonField.AsJsonObject() != nil {
				jsonObj := *jsonField.AsJsonObject()
				if objectField, exists := jsonObj["object"]; exists && objectField.IsJsonObject() {
					if objectField.AsJsonObject() != nil {
						objectObj := *objectField.AsJsonObject()
						if objectList, exists := objectObj["list"]; exists && objectList.IsJsonArray() {
							if objectList.AsJsonArray() != nil {
								array := *objectList.AsJsonArray()
								assert.Len(t, array, 3, "Expected nested array to have 3 elements")
							}
						}
					}
				}
			}
		}
	}
}