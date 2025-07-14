package main

import (
	"fmt"
)

func main() {
	fmt.Println("Recursive type tests are now available as Go tests.")
	fmt.Println("Run 'go test' to run all tests, or 'go test -run TestFoo' to run a specific test.")
	fmt.Println("")
	fmt.Println("Available tests:")
	fmt.Println("  TestFoo")
	fmt.Println("  TestJsonInput")
	fmt.Println("  TestFooStream")
}
