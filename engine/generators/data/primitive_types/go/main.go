package main

import (
	"fmt"
)

func main() {
	fmt.Println("Primitive type tests are now available as Go tests.")
	fmt.Println("Run 'go test' to run all tests, or 'go test -run TestPrimitiveTypes' to run a specific test.")
	fmt.Println("")
	fmt.Println("Available tests:")
	fmt.Println("  TestTopLevelString")
	fmt.Println("  TestTopLevelInt")
	fmt.Println("  TestTopLevelFloat")
	fmt.Println("  TestTopLevelBool")
	// fmt.Println("  TestTopLevelNull")  // TODO(vbv): Top level null is not supported yet
	fmt.Println("  TestPrimitiveTypes")
	fmt.Println("  TestPrimitiveArrays")
	fmt.Println("  TestPrimitiveMaps")
	fmt.Println("  TestMixedPrimitives")
	fmt.Println("  TestEmptyCollections")
}
