package main

import (
	"fmt"
)

func main() {
	fmt.Println("Union type tests are now available as Go tests.")
	fmt.Println("Run 'go test' to run all tests, or 'go test -run TestPrimitiveUnions' to run a specific test.")
	fmt.Println("")
	fmt.Println("Available tests:")
	fmt.Println("  TestPrimitiveUnions")
	fmt.Println("  TestComplexUnions")
	fmt.Println("  TestDiscriminatedUnions")
	fmt.Println("  TestUnionArrays")
}
