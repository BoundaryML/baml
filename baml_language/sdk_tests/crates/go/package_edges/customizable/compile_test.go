package sdk_test

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
	"baml.local/sdk/baml_sdk/packages/models"
)

var (
	_                                                                                                                                    = baml_sdk.EnumHolder{Status: models.StatusAlias(models.StatusReady)}
	_ func(context.Context, baml_sdk.EnumHolder) (baml_sdk.EnumHolder, error)                                                            = baml_sdk.RoundTripEnumHolder
	_ func(context.Context, models.StatusAlias) (models.StatusAlias, error)                                                              = baml_sdk.RoundTripStatusAlias
	_ func(context.Context, models.ThingAlias) (models.ThingAlias, error)                                                                = baml_sdk.RoundTripThingAlias
	_ func(context.Context, models.Thing) (models.Thing, error)                                                                          = baml_sdk.StaticFactoryRoundTripModel
	_ func(context.Context, models.ThingAlias) (models.ThingAlias, error)                                                                = baml_sdk.StaticFactoryRoundTripAlias
	_ func(context.Context, baml_sdk.Envelope) (baml_sdk.Envelope, error)                                                                = baml_sdk.StaticFactoryRoundTripNested
	_ func(context.Context, ...baml_sdk.StaticFactoryRoundTripEnumOption) (models.StatusAlias, error)                                    = baml_sdk.StaticFactoryRoundTripEnum
	_ func(models.StatusAlias) baml_sdk.StaticFactoryRoundTripEnumOption                                                                 = baml_sdk.WithStaticFactoryRoundTripEnumValue
	_                                                                                                                                    = baml_sdk.NewStringOrThingFromThing(models.Thing{Value: "cross-package"})
	_ func(context.Context, func(baml_sdk.StringOrThing) baml_sdk.StringOrThing, baml_sdk.StringOrThing) (baml_sdk.StringOrThing, error) = baml_sdk.CallCrossPackageUnionCallback
)

func Test_compile_cross_package_types_compile(t *testing.T) {
	t.Log("cross-package imports and collision-safe aliases compiled")
}

func Test_compile_llm_projection_default_overrides(t *testing.T) {
	// The synthetic fixture has tone: string = "neutral". Keep these calls
	// unreachable: this fixture has no bytecode, but Go must type-check the
	// non-null authored override independently for Spec and Stream.
	if false {
		_, _ = baml_sdk.DefaultedExtractSpec(
			context.Background(),
			"input",
			baml_sdk.DefaultedExtractSpecWithTone("spec override"),
		)
		_, _ = baml_sdk.DefaultedExtractStream(
			context.Background(),
			"input",
			baml_sdk.DefaultedExtractStreamWithTone("stream override"),
			baml_sdk.DefaultedExtractStreamClient("client override"),
			baml_sdk.DefaultedExtractStreamOnEvent(func(string) {}),
		)
	}
}
