package main

import (
	"fmt"
)

func main() {
	fmt.Println("Map types tests are now available as Go tests.")
	fmt.Println("Run 'go test' to run all tests, or 'go test -run TestSimpleMaps' to run a specific test.")
	fmt.Println("")
	fmt.Println("Available tests:")
	fmt.Println("  TestSimpleMaps")
	fmt.Println("  TestComplexMaps")
	fmt.Println("  TestNestedMaps")
	fmt.Println("  TestEdgeCaseMaps")
	fmt.Println("  TestLargeMaps")
	fmt.Println("  TestTopLevelStringMap")
	fmt.Println("  TestTopLevelIntMap")
	fmt.Println("  TestTopLevelFloatMap")
	fmt.Println("  TestTopLevelBoolMap")
	fmt.Println("  TestTopLevelNestedMap")
	fmt.Println("  TestTopLevelMapOfArrays")
	fmt.Println("  TestTopLevelEmptyMap")
	fmt.Println("  TestTopLevelMapWithNullable")
	fmt.Println("  TestTopLevelMapOfObjects")
}
