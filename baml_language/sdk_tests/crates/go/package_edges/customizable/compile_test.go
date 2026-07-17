package sdk_test

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
	"baml.local/sdk/baml_sdk/packages/models"
)

var (
	_                                                                         = baml_sdk.EnumHolder{Status: models.StatusAlias(models.StatusReady)}
	_ func(context.Context, baml_sdk.EnumHolder) (baml_sdk.EnumHolder, error) = baml_sdk.RoundTripEnumHolder
	_ func(context.Context, models.StatusAlias) (models.StatusAlias, error)   = baml_sdk.RoundTripStatusAlias
	_ func(context.Context, models.ThingAlias) (models.ThingAlias, error)     = baml_sdk.RoundTripThingAlias
)

func TestCrossPackageTypesCompile(t *testing.T) {
	t.Log("cross-package imports and collision-safe aliases compiled")
}
