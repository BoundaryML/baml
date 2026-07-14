package sdk_test

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
	"baml.local/sdk/baml_sdk/packages/models"
)

var (
	_                                                                         = baml_sdk.EnumHolder{Status: models.StatusReady}
	_ func(context.Context, baml_sdk.EnumHolder) (baml_sdk.EnumHolder, error) = baml_sdk.RoundTripEnumHolder
)

func TestCrossPackageTypesCompile(t *testing.T) {
	t.Log("cross-package imports and collision-safe aliases compiled")
}
