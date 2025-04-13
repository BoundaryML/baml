package main

import (
	"os"

	baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
)

func main() {
	// args from cli
	args := os.Args
	exit_code := baml.InvokeRuntimeCli(args)
	os.Exit(exit_code)
}
