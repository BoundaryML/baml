package sdk_test

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
	"baml.local/sdk/baml_sdk/packages/models"
)

var (
	_                                                                                                 = baml_sdk.EnumHolder{Status: models.StatusAlias(models.StatusReady)}
	_ func(context.Context, baml_sdk.EnumHolder) (baml_sdk.EnumHolder, error)                         = baml_sdk.RoundTripEnumHolder
	_ func(context.Context, models.StatusAlias) (models.StatusAlias, error)                           = baml_sdk.RoundTripStatusAlias
	_ func(context.Context, models.ThingAlias) (models.ThingAlias, error)                             = baml_sdk.RoundTripThingAlias
	_ func(context.Context, models.Thing) (models.Thing, error)                                       = baml_sdk.StaticFactoryRoundTripModel
	_ func(context.Context, models.ThingAlias) (models.ThingAlias, error)                             = baml_sdk.StaticFactoryRoundTripAlias
	_ func(context.Context, baml_sdk.Envelope) (baml_sdk.Envelope, error)                             = baml_sdk.StaticFactoryRoundTripNested
	_ func(context.Context, ...baml_sdk.StaticFactoryRoundTripEnumOption) (models.StatusAlias, error) = baml_sdk.StaticFactoryRoundTripEnum
	_ func(models.StatusAlias) baml_sdk.StaticFactoryRoundTripEnumOption                              = baml_sdk.WithStaticFactoryRoundTripEnumValue
)

func TestCrossPackageTypesCompile(t *testing.T) {
	t.Log("cross-package imports and collision-safe aliases compiled")
}
