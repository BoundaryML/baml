package main

import (
	"fmt"
)

func main() {
	fmt.Println("Union tests are now available as Go tests.")
	fmt.Println("Run 'go test' to run all tests, or 'go test -run TestJsonInputStream' to run a specific test.")
	fmt.Println("")
	fmt.Println("Available tests:")
	fmt.Println("  TestJsonInputStream")
	fmt.Println("  TestJsonInput")
}
