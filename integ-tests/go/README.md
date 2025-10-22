
`go mod tidy` - install deps

See also https://docs.boundaryml.com/guide/installation-language/go 

  Run all tests in current directory:
  go test

  Run all tests with verbose output:
  go test -v

  Run a specific test function:
  go test -run TestFunctionName

  Run tests in all subdirectories:
  go test ./...

  Since you have test_abort_handlers_test.go open, you could run just that test file with:
  go test -v -run TestAbortHandlers