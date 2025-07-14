package main

import (
	"fmt"
)

func main() {
	fmt.Println("Literal type tests are now available as Go tests.")
	fmt.Println("Run 'go test' to run all tests, or 'go test -run TestStringLiterals' to run a specific test.")
	fmt.Println("")
	fmt.Println("Available tests:")
	fmt.Println("  TestStringLiterals")
	fmt.Println("  TestIntegerLiterals")
	fmt.Println("  TestBooleanLiterals")
	fmt.Println("  TestMixedLiterals")
	fmt.Println("  TestComplexLiterals")
}
