package main

import (
	"fmt"
)

func main() {
	fmt.Println("Classes tests are now available as Go tests.")
	fmt.Println("Run 'go test' to run all tests, or 'go test -run TestConsumeSimpleClass' to run a specific test.")
	fmt.Println("")
	fmt.Println("Available tests:")
	fmt.Println("  TestConsumeSimpleClass")
	fmt.Println("  TestMakeSimpleClassStream")
}
