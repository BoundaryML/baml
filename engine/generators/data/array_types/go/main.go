package main

import (
	b "array_types/baml_client"
	"context"
	"fmt"
	"os"
)

func main() {
	ctx := context.Background()

	// Test simple arrays
	fmt.Println("Testing SimpleArrays...")
	simpleResult, err := b.TestSimpleArrays(ctx, "test simple arrays")
	if err != nil {
		fmt.Printf("Error testing simple arrays: %v\n", err)
		os.Exit(1)
	}

	// Verify simple array contents
	if len(simpleResult.Strings) != 3 {
		fmt.Printf("Expected strings length 3, got %d\n", len(simpleResult.Strings))
		os.Exit(1)
	}
	if len(simpleResult.Integers) != 5 {
		fmt.Printf("Expected integers length 5, got %d\n", len(simpleResult.Integers))
		os.Exit(1)
	}
	if len(simpleResult.Floats) != 3 {
		fmt.Printf("Expected floats length 3, got %d\n", len(simpleResult.Floats))
		os.Exit(1)
	}
	if len(simpleResult.Booleans) != 4 {
		fmt.Printf("Expected booleans length 4, got %d\n", len(simpleResult.Booleans))
		os.Exit(1)
	}
	fmt.Println("✓ SimpleArrays test passed")

	// Test nested arrays
	fmt.Println("\nTesting NestedArrays...")
	nestedResult, err := b.TestNestedArrays(ctx, "test nested arrays")
	if err != nil {
		fmt.Printf("Error testing nested arrays: %v\n", err)
		os.Exit(1)
	}

	// Verify nested array structure
	if len(nestedResult.Matrix) != 3 {
		fmt.Printf("Expected matrix length 3, got %d\n", len(nestedResult.Matrix))
		os.Exit(1)
	}
	if len(nestedResult.Matrix[0]) != 3 {
		fmt.Printf("Expected matrix[0] length 3, got %d\n", len(nestedResult.Matrix[0]))
		os.Exit(1)
	}
	if len(nestedResult.StringMatrix) != 2 {
		fmt.Printf("Expected stringMatrix length 2, got %d\n", len(nestedResult.StringMatrix))
		os.Exit(1)
	}
	if len(nestedResult.ThreeDimensional) != 2 {
		fmt.Printf("Expected threeDimensional length 2, got %d\n", len(nestedResult.ThreeDimensional))
		os.Exit(1)
	}
	fmt.Println("✓ NestedArrays test passed")

	// Test object arrays
	fmt.Println("\nTesting ObjectArrays...")
	objectResult, err := b.TestObjectArrays(ctx, "test object arrays")
	if err != nil {
		fmt.Printf("Error testing object arrays: %v\n", err)
		os.Exit(1)
	}

	// Verify object array contents
	if len(objectResult.Users) < 3 {
		fmt.Printf("Expected at least 3 users, got %d\n", len(objectResult.Users))
		os.Exit(1)
	}
	if len(objectResult.Products) < 2 {
		fmt.Printf("Expected at least 2 products, got %d\n", len(objectResult.Products))
		os.Exit(1)
	}
	if len(objectResult.Tags) < 4 {
		fmt.Printf("Expected at least 4 tags, got %d\n", len(objectResult.Tags))
		os.Exit(1)
	}

	// Verify user objects have required fields
	for i, user := range objectResult.Users {
		if user.Id <= 0 {
			fmt.Printf("User %d has invalid id: %d\n", i, user.Id)
			os.Exit(1)
		}
		if user.Name == "" {
			fmt.Printf("User %d has empty name\n", i)
			os.Exit(1)
		}
		if user.Email == "" {
			fmt.Printf("User %d has empty email\n", i)
			os.Exit(1)
		}
	}
	fmt.Println("✓ ObjectArrays test passed")

	// Test mixed arrays
	fmt.Println("\nTesting MixedArrays...")
	mixedResult, err := b.TestMixedArrays(ctx, "test mixed arrays")
	if err != nil {
		fmt.Printf("Error testing mixed arrays: %v\n", err)
		os.Exit(1)
	}

	// Verify mixed array contents
	if len(mixedResult.PrimitiveArray) != 4 {
		fmt.Printf("Expected primitiveArray length 4, got %d\n", len(mixedResult.PrimitiveArray))
		os.Exit(1)
	}
	if len(mixedResult.NullableArray) != 4 {
		fmt.Printf("Expected nullableArray length 4, got %d\n", len(mixedResult.NullableArray))
		os.Exit(1)
	}
	if len(mixedResult.OptionalItems) < 2 {
		fmt.Printf("Expected at least 2 optionalItems, got %d\n", len(mixedResult.OptionalItems))
		os.Exit(1)
	}
	if len(mixedResult.ArrayOfArrays) < 2 {
		fmt.Printf("Expected at least 2 arrayOfArrays, got %d\n", len(mixedResult.ArrayOfArrays))
		os.Exit(1)
	}
	if len(mixedResult.ComplexMixed) < 2 {
		fmt.Printf("Expected at least 2 complexMixed items, got %d\n", len(mixedResult.ComplexMixed))
		os.Exit(1)
	}
	fmt.Println("✓ MixedArrays test passed")

	// Test empty arrays
	fmt.Println("\nTesting EmptyArrays...")
	emptyResult, err := b.TestEmptyArrays(ctx, "test empty arrays")
	if err != nil {
		fmt.Printf("Error testing empty arrays: %v\n", err)
		os.Exit(1)
	}

	// Verify all arrays are empty
	if len(emptyResult.Strings) != 0 {
		fmt.Printf("Expected empty strings array, got length %d\n", len(emptyResult.Strings))
		os.Exit(1)
	}
	if len(emptyResult.Integers) != 0 {
		fmt.Printf("Expected empty integers array, got length %d\n", len(emptyResult.Integers))
		os.Exit(1)
	}
	if len(emptyResult.Floats) != 0 {
		fmt.Printf("Expected empty floats array, got length %d\n", len(emptyResult.Floats))
		os.Exit(1)
	}
	if len(emptyResult.Booleans) != 0 {
		fmt.Printf("Expected empty booleans array, got length %d\n", len(emptyResult.Booleans))
		os.Exit(1)
	}
	fmt.Println("✓ EmptyArrays test passed")

	// Test large arrays
	fmt.Println("\nTesting LargeArrays...")
	largeResult, err := b.TestLargeArrays(ctx, "test large arrays")
	if err != nil {
		fmt.Printf("Error testing large arrays: %v\n", err)
		os.Exit(1)
	}

	// Verify large array sizes
	if len(largeResult.Strings) < 40 {
		fmt.Printf("Expected at least 40 strings, got %d\n", len(largeResult.Strings))
		os.Exit(1)
	}
	if len(largeResult.Integers) < 50 {
		fmt.Printf("Expected at least 50 integers, got %d\n", len(largeResult.Integers))
		os.Exit(1)
	}
	if len(largeResult.Floats) < 20 {
		fmt.Printf("Expected at least 20 floats, got %d\n", len(largeResult.Floats))
		os.Exit(1)
	}
	if len(largeResult.Booleans) < 15 {
		fmt.Printf("Expected at least 15 booleans, got %d\n", len(largeResult.Booleans))
		os.Exit(1)
	}
	fmt.Println("✓ LargeArrays test passed")

	// Test top-level array return types
	fmt.Println("\nTesting top-level array return types...")

	// Test top-level string array
	stringArray, err := b.TestTopLevelStringArray(ctx, "test string array")
	if err != nil {
		fmt.Printf("Error testing top-level string array: %v\n", err)
		os.Exit(1)
	}
	if len(stringArray) != 4 {
		fmt.Printf("Expected 4 strings, got %d\n", len(stringArray))
		os.Exit(1)
	}
	if stringArray[0] != "apple" || stringArray[1] != "banana" || stringArray[2] != "cherry" || stringArray[3] != "date" {
		fmt.Printf("Unexpected values in string array\n")
		os.Exit(1)
	}
	fmt.Println("✓ TestTopLevelStringArray passed")

	// Test top-level int array
	intArray, err := b.TestTopLevelIntArray(ctx, "test int array")
	if err != nil {
		fmt.Printf("Error testing top-level int array: %v\n", err)
		os.Exit(1)
	}
	if len(intArray) != 5 {
		fmt.Printf("Expected 5 integers, got %d\n", len(intArray))
		os.Exit(1)
	}
	if intArray[0] != 10 || intArray[1] != 20 || intArray[2] != 30 || intArray[3] != 40 || intArray[4] != 50 {
		fmt.Printf("Unexpected values in int array\n")
		os.Exit(1)
	}
	fmt.Println("✓ TestTopLevelIntArray passed")

	// Test top-level float array
	floatArray, err := b.TestTopLevelFloatArray(ctx, "test float array")
	if err != nil {
		fmt.Printf("Error testing top-level float array: %v\n", err)
		os.Exit(1)
	}
	if len(floatArray) != 4 {
		fmt.Printf("Expected 4 floats, got %d\n", len(floatArray))
		os.Exit(1)
	}
	if floatArray[0] != 1.5 || floatArray[1] != 2.5 || floatArray[2] != 3.5 || floatArray[3] != 4.5 {
		fmt.Printf("Unexpected values in float array\n")
		os.Exit(1)
	}
	fmt.Println("✓ TestTopLevelFloatArray passed")

	// Test top-level bool array
	boolArray, err := b.TestTopLevelBoolArray(ctx, "test bool array")
	if err != nil {
		fmt.Printf("Error testing top-level bool array: %v\n", err)
		os.Exit(1)
	}
	if len(boolArray) != 5 {
		fmt.Printf("Expected 5 booleans, got %d\n", len(boolArray))
		os.Exit(1)
	}
	if !boolArray[0] || boolArray[1] || !boolArray[2] || boolArray[3] || !boolArray[4] {
		fmt.Printf("Unexpected values in bool array\n")
		os.Exit(1)
	}
	fmt.Println("✓ TestTopLevelBoolArray passed")

	// Test top-level nested array
	nestedArray, err := b.TestTopLevelNestedArray(ctx, "test nested array")
	if err != nil {
		fmt.Printf("Error testing top-level nested array: %v\n", err)
		os.Exit(1)
	}
	if len(nestedArray) != 3 {
		fmt.Printf("Expected 3 rows, got %d\n", len(nestedArray))
		os.Exit(1)
	}
	for i, row := range nestedArray {
		if len(row) != 3 {
			fmt.Printf("Expected 3 columns in row %d, got %d\n", i, len(row))
			os.Exit(1)
		}
	}
	fmt.Println("✓ TestTopLevelNestedArray passed")

	// Test top-level 3D array
	threeDArray, err := b.TestTopLevel3DArray(ctx, "test 3D array")
	if err != nil {
		fmt.Printf("Error testing top-level 3D array: %v\n", err)
		os.Exit(1)
	}
	if len(threeDArray) != 2 {
		fmt.Printf("Expected 2 levels, got %d\n", len(threeDArray))
		os.Exit(1)
	}
	for i, level := range threeDArray {
		if len(level) != 2 {
			fmt.Printf("Expected 2 rows in level %d, got %d\n", i, len(level))
			os.Exit(1)
		}
		for j, row := range level {
			if len(row) != 2 {
				fmt.Printf("Expected 2 columns in level %d row %d, got %d\n", i, j, len(row))
				os.Exit(1)
			}
		}
	}
	fmt.Println("✓ TestTopLevel3DArray passed")

	// Test top-level empty array
	emptyArray, err := b.TestTopLevelEmptyArray(ctx, "test empty array")
	if err != nil {
		fmt.Printf("Error testing top-level empty array: %v\n", err)
		os.Exit(1)
	}
	if len(emptyArray) != 0 {
		fmt.Printf("Expected empty array, got %d elements\n", len(emptyArray))
		os.Exit(1)
	}
	fmt.Println("✓ TestTopLevelEmptyArray passed")

	// Test top-level nullable array
	nullableArray, err := b.TestTopLevelNullableArray(ctx, "test nullable array")
	if err != nil {
		fmt.Printf("Error testing top-level nullable array: %v\n", err)
		os.Exit(1)
	}
	if len(nullableArray) != 5 {
		fmt.Printf("Expected 5 elements in nullable array, got %d\n", len(nullableArray))
		os.Exit(1)
	}
	if nullableArray[0] == nil || *nullableArray[0] != "hello" {
		fmt.Printf("Expected first element to be 'hello'\n")
		os.Exit(1)
	}
	if nullableArray[1] != nil {
		fmt.Printf("Expected second element to be nil\n")
		os.Exit(1)
	}
	fmt.Println("✓ TestTopLevelNullableArray passed")

	// Test top-level object array
	objectArray, err := b.TestTopLevelObjectArray(ctx, "test object array")
	if err != nil {
		fmt.Printf("Error testing top-level object array: %v\n", err)
		os.Exit(1)
	}
	if len(objectArray) != 3 {
		fmt.Printf("Expected 3 users, got %d\n", len(objectArray))
		os.Exit(1)
	}
	for i, user := range objectArray {
		if user.Name == "" || user.Email == "" {
			fmt.Printf("User %d has empty fields\n", i)
			os.Exit(1)
		}
	}
	fmt.Println("✓ TestTopLevelObjectArray passed")

	// Test top-level mixed array
	mixedArray, err := b.TestTopLevelMixedArray(ctx, "test mixed array")
	if err != nil {
		fmt.Printf("Error testing top-level mixed array: %v\n", err)
		os.Exit(1)
	}
	if len(mixedArray) != 6 {
		fmt.Printf("Expected 6 elements in mixed array, got %d\n", len(mixedArray))
		os.Exit(1)
	}
	fmt.Println("✓ TestTopLevelMixedArray passed")

	// Test top-level array of maps
	arrayOfMaps, err := b.TestTopLevelArrayOfMaps(ctx, "test array of maps")
	if err != nil {
		fmt.Printf("Error testing top-level array of maps: %v\n", err)
		os.Exit(1)
	}
	if len(arrayOfMaps) != 3 {
		fmt.Printf("Expected 3 maps in array, got %d\n", len(arrayOfMaps))
		os.Exit(1)
	}
	if len(arrayOfMaps[0]) != 2 || len(arrayOfMaps[1]) != 2 || len(arrayOfMaps[2]) != 2 {
		fmt.Printf("Unexpected map sizes in array of maps\n")
		os.Exit(1)
	}
	fmt.Println("✓ TestTopLevelArrayOfMaps passed")

	fmt.Println("\n✅ All array type tests passed!")
}
