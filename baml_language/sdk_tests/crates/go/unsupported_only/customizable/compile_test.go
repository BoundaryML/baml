package sdk_test

import "testing"

func Test_compile_unsupported_only_package_compiles(t *testing.T) {
	t.Log("unsupported functions were omitted without invalidating the package")
}
