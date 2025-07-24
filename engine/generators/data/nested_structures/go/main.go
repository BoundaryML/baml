package main

import (
	"fmt"
)

func main() {
	fmt.Println("Nested structure tests are now available as Go tests.")
	fmt.Println("Run 'go test' to run all tests, or 'go test -run TestSimpleNested' to run a specific test.")
	fmt.Println("")
	fmt.Println("Available tests:")
	fmt.Println("  TestSimpleNested")
	fmt.Println("  TestDeeplyNested")
	fmt.Println("  TestComplexNested")
	fmt.Println("  TestRecursiveStructure")
}
