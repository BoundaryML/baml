package main

import (
	"context"
	"testing"

	b "example.com/integ-tests/baml_client"
	"example.com/integ-tests/baml_client/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestSyncFunctionCall tests basic synchronous function calls
// Reference: test_functions.py:58-68
func TestSyncFunctionCall(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.TestFnNamedArgsSingleClass(ctx, types.NamedArgsSingleClass{
		Key:      "key",
		Key_two:  true,
		Key_three: 52,
	})
	
	require.NoError(t, err)
	assert.Contains(t, result, "52")
}

// TestSingleBoolInput tests single boolean input functions
// Reference: test_functions.py:72-74
func TestSingleBoolInput(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.TestFnNamedArgsSingleBool(ctx, true)
	require.NoError(t, err)
	assert.True(t, result == "true")
}

// TestSingleStringListInput tests single string list input
// Reference: test_functions.py:77-82
func TestSingleStringListInput(t *testing.T) {
	ctx := context.Background()
	
	// Test with items
	result, err := b.TestFnNamedArgsSingleStringList(ctx, []string{"a", "b", "c"})
	require.NoError(t, err)
	assert.Contains(t, result, "a")
	assert.Contains(t, result, "b") 
	assert.Contains(t, result, "c")
	
	// Test empty list
	result, err = b.TestFnNamedArgsSingleStringList(ctx, []string{})
	require.NoError(t, err)
	assert.Empty(t, result)
}

// TestMultipleArgsFunction tests functions with multiple arguments
// Reference: test_functions.py:140-153
func TestMultipleArgsFunction(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.TestMulticlassNamedArgs(ctx, 
		types.NamedArgsSingleClass{
			Key:      "key",
			Key_two:  true,
			Key_three: 52,
		},
		types.NamedArgsSingleClass{
			Key:      "key",
			Key_two:  true, 
			Key_three: 64,
		},
	)
	
	require.NoError(t, err)
	assert.Contains(t, result, "52")
	assert.Contains(t, result, "64")
}

// TestSingleEnumInput tests single enum input
// Reference: test_functions.py:156-158
func TestSingleEnumInput(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.TestFnNamedArgsSingleEnumList(ctx, []types.NamedArgsSingleEnumList{types.NamedArgsSingleEnumListTWO})
	require.NoError(t, err)
	assert.Contains(t, result, "TWO")
}

// TestSingleFloat tests single float input
// Reference: test_functions.py:161-163
func TestSingleFloat(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.TestFnNamedArgsSingleFloat(ctx, 3.12)
	require.NoError(t, err)
	assert.Contains(t, result, "3.12")
}

// TestSingleInt tests single integer input
// Reference: test_functions.py:165-167
func TestSingleInt(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.TestFnNamedArgsSingleInt(ctx, 3566)
	require.NoError(t, err)
	assert.Contains(t, result, "3566")
}

// TestSingleLiteralInt tests single literal integer
// Reference: test_functions.py:170-173
func TestSingleLiteralInt(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.TestNamedArgsLiteralInt(ctx, 1)
	require.NoError(t, err)
	assert.Contains(t, result, "1")
}

// TestSingleLiteralBool tests single literal boolean
// Reference: test_functions.py:175-178
func TestSingleLiteralBool(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.TestNamedArgsLiteralBool(ctx, true)
	require.NoError(t, err)
	assert.Contains(t, result, "true")
}

// TestSingleLiteralString tests single literal string
// Reference: test_functions.py:180-183
func TestSingleLiteralString(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.TestNamedArgsLiteralString(ctx, "My String")
	require.NoError(t, err)
	assert.Contains(t, result, "My String")
}

// TestOptionalStringInput tests optional string parameters
// Reference: test_functions.py (inferred from FnNamedArgsSingleStringOptional)
func TestOptionalStringInput(t *testing.T) {
	ctx := context.Background()
	
	// Test with value
	testString := "test value"
	result, err := b.FnNamedArgsSingleStringOptional(ctx, &testString)
	require.NoError(t, err)
	assert.Contains(t, result, "test value")
	
	// Test with nil
	result, err = b.FnNamedArgsSingleStringOptional(ctx, nil)
	require.NoError(t, err)
	// Should still return some result when passed nil
	assert.NotEmpty(t, result)
}

// TestAllOutputTypes tests various output types
// Reference: test_functions.py:384-418
func TestAllOutputTypes(t *testing.T) {
	ctx := context.Background()
	testInput := "test input"
	
	// Test bool output
	boolResult, err := b.FnOutputBool(ctx, testInput)
	require.NoError(t, err)
	assert.True(t, boolResult)
	
	// Test int output
	intResult, err := b.FnOutputInt(ctx, testInput)
	require.NoError(t, err)
	assert.Equal(t, int64(5), intResult)
	
	// Test literal int output
	literalIntResult, err := b.FnOutputLiteralInt(ctx, testInput)
	require.NoError(t, err)
	assert.Equal(t, int64(5), literalIntResult)
	
	// Test literal bool output
	literalBoolResult, err := b.FnOutputLiteralBool(ctx, testInput)
	require.NoError(t, err)
	assert.False(t, literalBoolResult)
	
	// Test literal string output
	literalStringResult, err := b.FnOutputLiteralString(ctx, testInput)
	require.NoError(t, err)
	assert.Equal(t, "example output", literalStringResult)
	
	// Test class list output
	classListResult, err := b.FnOutputClassList(ctx, testInput)
	require.NoError(t, err)
	assert.NotEmpty(t, classListResult)
	assert.NotEmpty(t, classListResult[0].Prop1)
	
	// Test class with enum output
	classWithEnumResult, err := b.FnOutputClassWithEnum(ctx, testInput)
	require.NoError(t, err)
	assert.Contains(t, []string{"ONE", "TWO"}, string(classWithEnumResult.Prop2))
	
	// Test class output
	classResult, err := b.FnOutputClass(ctx, testInput)
	require.NoError(t, err)
	assert.NotEmpty(t, classResult.Prop1)
	assert.Equal(t, int64(540), classResult.Prop2)
	
	// Test enum list output
	enumListResult, err := b.FnEnumListOutput(ctx, "pick 2 at random")
	require.NoError(t, err)
	assert.Len(t, enumListResult, 2)
	
	// Test enum output
	enumResult, err := b.FnEnumOutput(ctx, "pick the last option")
	require.NoError(t, err)
	assert.NotEmpty(t, enumResult)
}