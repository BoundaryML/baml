package main

import (
	"fmt"
)

func main() {
	fmt.Println("Edge case tests are now available as Go tests.")
	fmt.Println("Run 'go test' to run all tests, or 'go test -run TestEmptyCollections' to run a specific test.")
	fmt.Println("")
	fmt.Println("Available tests:")
	fmt.Println("  TestEmptyCollections")
	fmt.Println("  TestLargeStructure")
	fmt.Println("  TestDeepRecursion")
	fmt.Println("  TestSpecialCharacters")
	fmt.Println("  TestNumberEdgeCases")
	fmt.Println("  TestCircularReference")
}
