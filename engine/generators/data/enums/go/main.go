package main

import (
	"fmt"
)

func main() {
	fmt.Println("Enum tests are now available as Go tests.")
	fmt.Println("Run 'go test' to run all tests, or 'go test -run TestConsumeTestEnum' to run a specific test.")
	fmt.Println("")
	fmt.Println("Available tests:")
	fmt.Println("  TestConsumeTestEnum")
	fmt.Println("  TestFnTestAliasedEnumOutput")
	fmt.Println("  TestFnTestAliasedEnumOutputVariants")
}
