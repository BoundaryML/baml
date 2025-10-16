package main

import (
	"fmt"
)

func main() {
	fmt.Println("Semantic streaming tests are now available as Go tests.")
	fmt.Println("Run 'go test' to run all tests, or 'go test -run TestMakeSemanticContainerStream' to run a specific test.")
	fmt.Println("")
	fmt.Println("Available tests:")
	fmt.Println("  TestMakeSemanticContainerStream")
	fmt.Println("  TestMakeSemanticContainer")
}
