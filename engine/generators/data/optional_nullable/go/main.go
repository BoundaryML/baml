package main

import (
	"fmt"
)

func main() {
	fmt.Println("Optional/nullable type tests are now available as Go tests.")
	fmt.Println("Run 'go test' to run all tests, or 'go test -run TestOptionalFields' to run a specific test.")
	fmt.Println("")
	fmt.Println("Available tests:")
	fmt.Println("  TestOptionalFields")
	fmt.Println("  TestNullableTypes")
	fmt.Println("  TestMixedOptionalNullable")
	fmt.Println("  TestAllNull")
	fmt.Println("  TestAllOptionalOmitted")
}
