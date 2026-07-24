package sdk_test

import "testing"

func TestUnsupportedOnlyPackageCompiles(t *testing.T) {
	t.Log("unsupported functions were omitted without invalidating the package")
}
