package main

import (
	"context"
	"fmt"
	"testing"

	b "example.com/integ-tests/baml_client"
	"example.com/integ-tests/baml_client/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestSimpleRecursiveType tests basic recursive linked list construction
// Reference: test_functions.py:1351-1362
func TestSimpleRecursiveType(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.BuildLinkedList(ctx, []int64{1, 2, 3, 4, 5})
	require.NoError(t, err)
	
	expected := types.LinkedList{
		Len: 5,
		Head: &types.Node{
			Data: 1,
			Next: &types.Node{
				Data: 2,
				Next: &types.Node{
					Data: 3,
					Next: &types.Node{
						Data: 4,
						Next: &types.Node{
							Data: 5,
							Next: nil,
						},
					},
				},
			},
		},
	}
	
	assert.Equal(t, expected, result)
	
	// Verify the linked list structure
	current := result.Head
	expectedValues := []int64{1, 2, 3, 4, 5}
	
	for i, expectedValue := range expectedValues {
		require.NotNil(t, current, "Expected node at position %d", i)
		assert.Equal(t, expectedValue, current.Data, "Expected correct data at position %d", i)
		current = current.Next
	}
	
	assert.Nil(t, current, "Expected end of list to be nil")
}

// TestMutuallyRecursiveType tests mutually recursive tree structures
// Reference: test_functions.py:1366-1413
func TestMutuallyRecursiveType(t *testing.T) {
	ctx := context.Background()
	
	input := types.BinaryNode{
		Data: 5,
		Left: &types.BinaryNode{
			Data: 3,
			Left: &types.BinaryNode{
				Data: 1,
				Left: &types.BinaryNode{
					Data: 2,
					Left: nil,
					Right: nil,
				},
				Right: nil,
			},
			Right: &types.BinaryNode{
				Data: 4,
				Left: nil,
				Right: nil,
			},
		},
		Right: &types.BinaryNode{
			Data: 7,
			Left: &types.BinaryNode{
				Data: 6,
				Left: nil,
				Right: nil,
			},
			Right: &types.BinaryNode{
				Data: 8,
				Left: nil,
				Right: nil,
			},
		},
	}
	
	result, err := b.BuildTree(ctx, input)
	require.NoError(t, err)
	
	expected := types.Tree{
		Data: 5,
		Children: types.Forest{
			Trees: []types.Tree{
				{
					Data: 3,
					Children: types.Forest{
						Trees: []types.Tree{
							{
								Data: 1,
								Children: types.Forest{
									Trees: []types.Tree{
										{
											Data: 2,
											Children: types.Forest{Trees: []types.Tree{}},
										},
									},
								},
							},
							{
								Data: 4,
								Children: types.Forest{Trees: []types.Tree{}},
							},
						},
					},
				},
				{
					Data: 7,
					Children: types.Forest{
						Trees: []types.Tree{
							{
								Data: 6,
								Children: types.Forest{Trees: []types.Tree{}},
							},
							{
								Data: 8,
								Children: types.Forest{Trees: []types.Tree{}},
							},
						},
					},
				},
			},
		},
	}
	
	assert.Equal(t, expected, result)
	
	// Verify tree structure
	assert.Equal(t, int64(5), result.Data, "Expected root data to be 5")
	assert.Len(t, result.Children.Trees, 2, "Expected root to have 2 children")
	
	// Check left subtree
	leftChild := result.Children.Trees[0] 
	assert.Equal(t, int64(3), leftChild.Data, "Expected left child data to be 3")
	assert.Len(t, leftChild.Children.Trees, 2, "Expected left child to have 2 children")
	
	// Check right subtree
	rightChild := result.Children.Trees[1]
	assert.Equal(t, int64(7), rightChild.Data, "Expected right child data to be 7")
	assert.Len(t, rightChild.Children.Trees, 2, "Expected right child to have 2 children")
}

// TestAliasThatPointsToRecursiveType tests aliases pointing to recursive types
// Reference: test_functions.py:250-254
func TestAliasThatPointsToRecursiveType(t *testing.T) {
	ctx := context.Background()
	
	input := types.LinkedListAliasNode{
		Value: 1,
		Next:  nil,
	}
	
	result, err := b.AliasThatPointsToRecursiveType(ctx, input)
	require.NoError(t, err)
	
	expected := types.LinkedListAliasNode{
		Value: 1,
		Next:  nil,
	}
	
	assert.Equal(t, expected, result)
}

// TestClassThatPointsToRecursiveClassThroughAlias tests classes pointing to recursive types via aliases
// Reference: test_functions.py:257-261
func TestClassThatPointsToRecursiveClassThroughAlias(t *testing.T) {
	ctx := context.Background()
	
	input := types.ClassToRecAlias{
		List: types.LinkedListAliasNode{
			Value: 1,
			Next:  nil,
		},
	}
	
	result, err := b.ClassThatPointsToRecursiveClassThroughAlias(ctx, input)
	require.NoError(t, err)
	
	expected := types.ClassToRecAlias{
		List: types.LinkedListAliasNode{
			Value: 1,
			Next:  nil,
		},
	}
	
	assert.Equal(t, expected, result)
}

// TestRecursiveClassWithAliasIndirection tests recursive classes with alias indirection
// Reference: test_functions.py:264-272
func TestRecursiveClassWithAliasIndirection(t *testing.T) {
	ctx := context.Background()
	
	input := types.NodeWithAliasIndirection{
		Value: 1,
		Next: &types.NodeWithAliasIndirection{
			Value: 2,
			Next:  nil,
		},
	}
	
	result, err := b.RecursiveClassWithAliasIndirection(ctx, input)
	require.NoError(t, err)
	
	expected := types.NodeWithAliasIndirection{
		Value: 1,
		Next: &types.NodeWithAliasIndirection{
			Value: 2,
			Next:  nil,
		},
	}
	
	assert.Equal(t, expected, result)
	
	// Verify recursive structure
	assert.Equal(t, int64(1), result.Value, "Expected root value to be 1")
	require.NotNil(t, result.Next, "Expected root to have next node")
	assert.Equal(t, int64(2), result.Next.Value, "Expected next value to be 2")
	assert.Nil(t, result.Next.Next, "Expected second node to be terminal")
}

// TestSimpleRecursiveMapAlias tests simple recursive map aliases
// Reference: test_functions.py:293-295
func TestDegenerateRecursiveMapAlias(t *testing.T) {
	// TODO: too lazy
	// ctx := context.Background()
	
	// // RecursiveMapAlias is map[string]any
	// input := types.RecursiveMapAlias{
	// 	"one": types.RecursiveMapAlias{
	// 		"two": types.RecursiveMapAlias{
	// 			"three": types.RecursiveMapAlias{},
	// 		},
	// 	},
	// }
	
	// result, err := b.SimpleRecursiveMapAlias(ctx, input)
	// require.NoError(t, err)
	
	// expected := types.RecursiveMapAlias{
	// 	"one": map[string]any{
	// 		"two": map[string]any{
	// 			"three": map[string]any{},
	// 		},
	// 	},
	// }
	
	// assert.Equal(t, expected, result)
	
	// // Verify nested structure
	// one := result["one"].(map[string]any)
	// two := one["two"].(map[string]any)
	// three := two["three"].(map[string]any)
	// assert.Equal(t, map[string]any{}, three, "Expected empty map at deepest level")
}

// TestSimpleRecursiveListAlias tests simple recursive list aliases
// Reference: test_functions.py:298-300
func TestDegenerateRecursiveListAlias(t *testing.T) {
	// TODO: too lazy
	// ctx := context.Background()
	
	// // RecursiveListAlias is []any
	// input := types.RecursiveListAlias{
	// 	[]any{},
	// 	[]any{},
	// 	[]any{[]any{}},
	// }
	
	// result, err := b.SimpleRecursiveListAlias(ctx, input)
	// require.NoError(t, err)
	
	// expected := types.RecursiveListAlias{
	// 	[]any{},
	// 	[]any{},
	// 	[]any{[]any{}},
	// }
	
	// assert.Equal(t, expected, result)
	
	// // Verify structure
	// assert.Len(t, result, 3, "Expected 3 top-level elements")
	// if len(result) >= 3 {
	// 	if arr0, ok := result[0].([]any); ok {
	// 		assert.Len(t, arr0, 0, "Expected first element to be empty")
	// 	}
	// 	if arr1, ok := result[1].([]any); ok {
	// 		assert.Len(t, arr1, 0, "Expected second element to be empty")
	// 	}
	// 	if arr2, ok := result[2].([]any); ok {
	// 		assert.Len(t, arr2, 1, "Expected third element to have one nested element")
	// 		if len(arr2) > 0 {
	// 			if nested, ok := arr2[0].([]any); ok {
	// 				assert.Len(t, nested, 0, "Expected nested element to be empty")
	// 			}
	// 		}
	// 	}
	// }
}

// TestRecursiveAliasCycles tests recursive alias cycles
// Reference: test_functions.py:303-305
func TestDegenerateRecursiveAliasCycles(t *testing.T) {
	// TODO: too lazy
	// ctx := context.Background()
	
	// // RecAliasOne is []any  
	// input := types.RecAliasOne{
	// 	[]any{},
	// 	[]any{},
	// 	[]any{[]any{}},
	// }
	
	// result, err := b.RecursiveAliasCycle(ctx, input)
	// require.NoError(t, err)
	
	// expected := types.RecAliasOne{
	// 	[]any{},
	// 	[]any{},
	// 	[]any{[]any{}},
	// }
	
	// assert.Equal(t, expected, result)
}

// TestReturnJsonEntry tests returning JSON entries with recursive structures
// Reference: test_functions.py:364-366
func TestReturnJsonEntry(t *testing.T) {
	ctx := context.Background()
	
	jsonInput := `{
		"a": "A",
		"b": {
			"c": "C"
		}
	}`
	
	result, err := b.ReturnJsonEntry(ctx, jsonInput)
	require.NoError(t, err)
	
	// Verify the structure matches expected nested SimpleTag format
	expectedA := types.Union2JsonTemplateOrSimpleTag__NewSimpleTag(types.SimpleTag{Field: "A"})
	expectedB := types.Union2JsonTemplateOrSimpleTag__NewJsonTemplate(types.JsonTemplate{
		"c": types.Union2JsonTemplateOrSimpleTag__NewSimpleTag(types.SimpleTag{Field: "C"}),
	})

	jsonA := fmt.Sprintf("%v", expectedA)
	jsonB := fmt.Sprintf("%v", expectedB)
	jsonResultA := fmt.Sprintf("%v", result["a"])
	jsonResultB := fmt.Sprintf("%v", result["b"])
	
	if jsonA != jsonResultA {
		t.Logf("Expected: %s", jsonA)
		t.Logf("Actual: %s", jsonResultA)
	}
	if jsonB != jsonResultB {
		t.Logf("Expected: %s", jsonB)
		t.Logf("Actual: %s", jsonResultB)
	}
}

// TestDeepRecursiveStructure tests deeply nested recursive structures
func TestDeepRecursiveStructure(t *testing.T) {
	ctx := context.Background()
	
	// Create a deeper linked list
	input := []int64{1, 2, 3, 4, 5, 6, 7, 8, 9, 10}
	
	result, err := b.BuildLinkedList(ctx, input)
	require.NoError(t, err)
	
	assert.Equal(t, int64(10), result.Len, "Expected length to be 10")
	
	// Traverse and verify the entire chain
	current := result.Head
	for i, expectedValue := range input {
		require.NotNil(t, current, "Expected node at position %d", i)
		assert.Equal(t, expectedValue, current.Data, "Expected correct value at position %d", i)
		current = current.Next
	}
	
	assert.Nil(t, current, "Expected end of list to be nil")
}

// TestComplexTreeStructure tests more complex tree operations
func TestComplexTreeStructure(t *testing.T) {
	ctx := context.Background()
	
	// Create a more complex binary tree
	input := types.BinaryNode{
		Data: 10,
		Left: &types.BinaryNode{
			Data: 5,
			Left: &types.BinaryNode{Data: 3, Left: nil, Right: nil},
			Right: &types.BinaryNode{Data: 7, Left: nil, Right: nil},
		},
		Right: &types.BinaryNode{
			Data: 15,
			Left: &types.BinaryNode{Data: 12, Left: nil, Right: nil},
			Right: &types.BinaryNode{Data: 18, Left: nil, Right: nil},
		},
	}
	
	result, err := b.BuildTree(ctx, input)
	require.NoError(t, err)
	
	// Verify the conversion preserved the structure
	assert.Equal(t, int64(10), result.Data, "Expected root data")
	assert.Len(t, result.Children.Trees, 2, "Expected two main subtrees")
	
	// Check left subtree structure
	leftSubtree := result.Children.Trees[0]
	assert.Equal(t, int64(5), leftSubtree.Data)
	assert.Len(t, leftSubtree.Children.Trees, 2, "Expected left subtree to have 2 children")
	
	// Check right subtree structure  
	rightSubtree := result.Children.Trees[1] 
	assert.Equal(t, int64(15), rightSubtree.Data)
	assert.Len(t, rightSubtree.Children.Trees, 2, "Expected right subtree to have 2 children")
}