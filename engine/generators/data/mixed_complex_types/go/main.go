package main

import (
	"fmt"
)

func main() {
	fmt.Println("Mixed complex type tests are now available as Go tests.")
	fmt.Println("Run 'go test' to run all tests, or 'go test -run TestKitchenSink' to run a specific test.")
	fmt.Println("")
	fmt.Println("Available tests:")
	fmt.Println("  TestKitchenSink")
	fmt.Println("  TestUltraComplex")
	fmt.Println("  TestRecursiveComplexity")
}
