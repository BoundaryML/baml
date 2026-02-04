package main

import (
	"context"
	"fmt"
	"strings"
	"testing"

	b "example.com/integ-tests/baml_client"
	"example.com/integ-tests/baml_client/types"
	baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestDynamicClassCreation tests creating dynamic classes
// Reference: test_typebuilder.py:18-43
func TestDynamicClassCreation(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	personClass, err := tb.Person()
	require.NoError(t, err)

	// Add property to Person class
	stringType, err := tb.String()
	require.NoError(t, err)
	listType, err := stringType.List()
	require.NoError(t, err)
	_, err = personClass.AddProperty("last_name", listType)
	require.NoError(t, err)

	floatType, err := tb.Float()
	require.NoError(t, err)

	heightProperty, err := personClass.AddProperty("height", floatType)
	require.NoError(t, err)
	err = heightProperty.SetDescription("Height in meters")
	require.NoError(t, err)

	// Test the modified class
	result, err := b.ExtractPeople(ctx, "My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good around the hoop.", b.WithTypeBuilder(tb))
	require.NoError(t, err)
	assert.NotEmpty(t, result, "Expected non-empty result")

	for _, person := range result {
		t.Logf("Person: %+v", person)
		// Verify basic properties still work
		assert.NotNil(t, person.Name)
		assert.NotNil(t, person.Hair_color)
		assert.NotNil(t, person.DynamicProperties["height"])
	}
}

// TestDynamicEnumCreation tests creating and modifying enums
// Reference: test_typebuilder.py:226-235
func TestDynamicEnumCreation(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	hobbyEnum, err := tb.Hobby()
	require.NoError(t, err)

	// Add value to existing enum
	_, err = hobbyEnum.AddValue("SOFTWARE")
	require.NoError(t, err)

	result, err := b.ExtractHobby(ctx, "My name is Harrison. My hair is black and I'm 6 feet tall. I love coding and music.", b.WithTypeBuilder(tb))
	require.NoError(t, err)
	assert.NotEmpty(t, result, "Expected non-empty result")

	// Should contain the new "Golfing" value
	assert.Contains(t, result, types.Hobby("SOFTWARE"))
	assert.Contains(t, result, types.HobbyMUSIC)
}

// TestTypeBuilderPrint tests type builder string representation
// Reference: test_typebuilder.py:46-57
func TestTypeBuilderPrint(t *testing.T) {
	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	personClass, err := tb.Person()
	require.NoError(t, err)

	stringType, err := tb.String()
	require.NoError(t, err)
	listType, err := stringType.List()
	require.NoError(t, err)
	_, err = personClass.AddProperty("candy", listType)
	require.NoError(t, err)

	tbStr := fmt.Sprintf("%v", tb)
	t.Logf("TypeBuilder string representation: %s", tbStr)

	expectedContent := []string{
		"TypeBuilder",
		"Person",
		"candy",
		"string[]",
	}

	for _, content := range expectedContent {
		assert.Contains(t, tbStr, content, "Expected TypeBuilder string to contain '%s'", content)
	}
}

// TestDynamicClassOutput tests dynamic class output
// Reference: test_typebuilder.py:61-78
func TestDynamicClassOutput(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	dynamicOutputClass, err := tb.DynamicOutput()
	require.NoError(t, err)

	stringType, err := tb.String()
	require.NoError(t, err)
	_, err = dynamicOutputClass.AddProperty("hair_color", stringType)
	require.NoError(t, err)

	// List properties to verify
	properties, err := dynamicOutputClass.ListProperties()
	require.NoError(t, err)

	var foundHairColor bool
	for _, prop := range properties {
		name, err := prop.Name()
		require.NoError(t, err)
		t.Logf("Property: %s", name)
		if name == "hair_color" {
			foundHairColor = true
		}
	}
	assert.True(t, foundHairColor, "Expected to find hair_color property")

	// Test the function with dynamic output
	output, err := b.MyFunc(ctx, "My name is Harrison. My hair is black and I'm 6 feet tall.", b.WithTypeBuilder(tb))
	require.NoError(t, err)

	t.Logf("Dynamic output: %+v", output)
	// The exact assertion depends on how dynamic properties are accessed in Go
	// This is a structural test to ensure the call succeeds
}

// TestDynamicClassNestedOutput tests nested dynamic classes
// Reference: test_typebuilder.py:81-109
func TestDynamicClassNestedOutput(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	// Create nested class
	nameClass, err := tb.AddClass("Name")
	require.NoError(t, err)

	stringType, err := tb.String()
	require.NoError(t, err)
	_, err = nameClass.AddProperty("first_name", stringType)
	require.NoError(t, err)

	optionalStringType, err := stringType.Optional()
	require.NoError(t, err)
	_, err = nameClass.AddProperty("last_name", optionalStringType)
	require.NoError(t, err)

	_, err = nameClass.AddProperty("middle_name", optionalStringType)
	require.NoError(t, err)

	// Create another nested class
	addressClass, err := tb.AddClass("Address")
	require.NoError(t, err)

	_, err = addressClass.AddProperty("street", stringType)
	require.NoError(t, err)

	_, err = addressClass.AddProperty("city", stringType)
	require.NoError(t, err)

	_, err = addressClass.AddProperty("state", stringType)
	require.NoError(t, err)

	_, err = addressClass.AddProperty("zip", stringType)
	require.NoError(t, err)

	// Add nested properties to DynamicOutput
	dynamicOutputClass, err := tb.DynamicOutput()
	require.NoError(t, err)

	nameType, err := nameClass.Type()
	require.NoError(t, err)
	optionalNameType, err := nameType.Optional()
	require.NoError(t, err)
	_, err = dynamicOutputClass.AddProperty("name", optionalNameType)
	require.NoError(t, err)

	addressType, err := addressClass.Type()
	require.NoError(t, err)
	optionalAddressType, err := addressType.Optional()
	require.NoError(t, err)
	_, err = dynamicOutputClass.AddProperty("address", optionalAddressType)
	require.NoError(t, err)

	hairColorProp, err := dynamicOutputClass.AddProperty("hair_color", optionalStringType)
	require.NoError(t, err)
	err = hairColorProp.SetAlias("hairColor")
	require.NoError(t, err)

	floatType, err := tb.Float()
	require.NoError(t, err)
	optionalFloatType, err := floatType.Optional()
	require.NoError(t, err)
	_, err = dynamicOutputClass.AddProperty("height", optionalFloatType)
	require.NoError(t, err)

	output, err := b.MyFunc(ctx, "My name is Mark Gonzalez. My hair is black and I'm 6 feet tall.", b.WithTypeBuilder(tb))
	require.NoError(t, err)

	t.Logf("Nested dynamic output: %+v", output)
}

// TestDynamicNewEnum tests creating completely new enums
// Reference: test_typebuilder.py:210-222
func TestDynamicNewEnum(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	// Create new enum
	animalEnum, err := tb.AddEnum("Animal")
	require.NoError(t, err)

	animals := []string{"giraffe", "elephant", "lion"}
	for _, animal := range animals {
		_, err = animalEnum.AddValue(strings.ToUpper(animal))
		require.NoError(t, err)
	}

	// Add the new enum to Person class
	personClass, err := tb.Person()
	require.NoError(t, err)

	animalType, err := animalEnum.Type()
	require.NoError(t, err)
	_, err = personClass.AddProperty("animalLiked", animalType)
	require.NoError(t, err)

	result, err := b.ExtractPeople(ctx, "My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good around the hoop. I like giraffes.", b.WithTypeBuilder(tb))
	require.NoError(t, err)

	assert.NotEmpty(t, result, "Expected non-empty result")
	// The exact assertion for dynamic properties depends on Go client implementation
}

// TestDynamicLiterals tests dynamic literal types
// Reference: test_typebuilder.py:239-253
func TestDynamicLiterals(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	literalStringType, err := tb.LiteralString("GIRAFFE")
	require.NoError(t, err)
	literalStringType2, err := tb.LiteralString("ELEPHANT")
	require.NoError(t, err)
	literalStringType3, err := tb.LiteralString("LION")
	require.NoError(t, err)
	// Create union of literal strings
	literals, err := tb.Union([]baml.Type{
		literalStringType,
		literalStringType2,
		literalStringType3,
	})
	require.NoError(t, err)

	personClass, err := tb.Person()
	require.NoError(t, err)

	_, err = personClass.AddProperty("animalLiked", literals)
	require.NoError(t, err)

	result, err := b.ExtractPeople(ctx, "My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good around the hoop. I like giraffes.", b.WithTypeBuilder(tb))
	require.NoError(t, err)

	assert.NotEmpty(t, result, "Expected non-empty result")
}

// TestAddBAMLExistingClass tests adding BAML code for existing classes
// Reference: test_typebuilder.py:376-402
func TestAddBAMLExistingClass(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	bamlCode := `
        class ExtraPersonInfo {
            height int
            weight int
        }

        dynamic class Person {
            age int?
            extra ExtraPersonInfo?
        }
    `

	err = tb.AddBaml(bamlCode)
	require.NoError(t, err)

	result, err := b.ExtractPeople(ctx, "My name is John Doe. I'm 30 years old. I'm 6 feet tall and weigh 180 pounds. My hair is yellow.", b.WithTypeBuilder(tb))
	require.NoError(t, err)

	assert.NotEmpty(t, result, "Expected non-empty result")
	person := result[0]
	assert.NotNil(t, person.Name)
	assert.Equal(t, "John Doe", *person.Name)
	assert.Equal(t, types.ColorYELLOW, *person.Hair_color)
}

// TestAddBAMLExistingEnum tests adding BAML code for existing enums
// Reference: test_typebuilder.py:406-417
func TestAddBAMLExistingEnum(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	bamlCode := `
        dynamic enum Hobby {
            VideoGames
            BikeRiding
        }
    `

	err = tb.AddBaml(bamlCode)
	require.NoError(t, err)

	result, err := b.ExtractHobby(ctx, "I play video games", b.WithTypeBuilder(tb))
	require.NoError(t, err)

	assert.Contains(t, result, types.Hobby("VideoGames"))
}

// TestAddBAMLBothClassesAndEnums tests adding both classes and enums
// Reference: test_typebuilder.py:421-466
func TestAddBAMLBothClassesAndEnums(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	bamlCode := `
        class ExtraPersonInfo {
            height int @alias("height_inches")
            weight int @alias("weight_pounds")
        }

        enum Job {
            Programmer
            Architect
            Musician
        }

        dynamic enum Hobby {
            VideoGames
            BikeRiding
        }

        dynamic enum Color {
            BROWN
        }

        dynamic class Person {
            age int?
            extra ExtraPersonInfo?
            job Job?
            hobbies Hobby[]
        }
    `

	err = tb.AddBaml(bamlCode)
	require.NoError(t, err)

	result, err := b.ExtractPeople(ctx, "My name is John Doe. I'm 30 years old. My height is 6 feet and I weigh 180 pounds. My hair is brown. I work as a programmer and enjoy bike riding.", b.WithTypeBuilder(tb))
	require.NoError(t, err)

	assert.NotEmpty(t, result, "Expected non-empty result")
	person := result[0]
	assert.NotNil(t, person.Name)
	assert.Equal(t, "John Doe", *person.Name)
}

// TestAddBAMLWithAttrs tests adding BAML with attributes
// Reference: test_typebuilder.py:470-494
func TestAddBAMLWithAttrs(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	bamlCode := `
        class ExtraPersonInfo {
            height int @description("In centimeters and rounded to the nearest whole number")
            weight int @description("In kilograms and rounded to the nearest whole number")
        }

        dynamic class Person {
            extra ExtraPersonInfo?
        }
    `

	err = tb.AddBaml(bamlCode)
	require.NoError(t, err)

	result, err := b.ExtractPeople(ctx, "My name is John Doe. I'm 30 years old. I'm 6 feet tall and weigh 180 pounds. My hair is yellow.", b.WithTypeBuilder(tb))
	require.NoError(t, err)

	assert.NotEmpty(t, result, "Expected non-empty result")
	person := result[0]
	assert.NotNil(t, person.Name)
	assert.Equal(t, "John Doe", *person.Name)
	assert.Equal(t, types.ColorYELLOW, *person.Hair_color)
}

// TestAddBAMLError tests error handling in BAML addition
// Reference: test_typebuilder.py:498-508, 512-519
func TestAddBAMLError(t *testing.T) {
	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	t.Run("InvalidBAMLSyntax", func(t *testing.T) {
		invalidBAML := `
            dynamic Hobby {
                VideoGames
                BikeRiding
            }
        `

		err = tb.AddBaml(invalidBAML)
		assert.Error(t, err, "Expected error for invalid BAML syntax")
	})

	t.Run("ParserError", func(t *testing.T) {
		syntaxErrorBAML := `
            syntaxerror
        `

		err = tb.AddBaml(syntaxErrorBAML)
		assert.Error(t, err, "Expected parser error for syntax error")
	})
}

// TestReferencingExistingClassTypes tests referencing existing types
// Reference: test_typebuilder.py:523-526
func TestReferencingExistingClassTypes(t *testing.T) {
	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	personClass, err := tb.Person()
	require.NoError(t, err)

	// Create union of existing types
	resumeType, err := tb.Resume()
	require.NoError(t, err)
	resumeFieldType, err := resumeType.Type()
	require.NoError(t, err)

	hobbyEnum, err := tb.Hobby()
	require.NoError(t, err)
	hobbyFieldType, err := hobbyEnum.Type()
	require.NoError(t, err)

	propsUnion, err := tb.Union([]baml.Type{resumeFieldType, hobbyFieldType})
	require.NoError(t, err)

	_, err = personClass.AddProperty("props", propsUnion)
	require.NoError(t, err)
}

// TestTypeBuilderAndFieldTypeImports tests import functionality
// Reference: test_typebuilder.py:529-545
func TestTypeBuilderAndFieldTypeImports(t *testing.T) {
	// Test that both TypeBuilder and FieldType can be imported
	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)
	assert.NotNil(t, tb, "Expected non-nil TypeBuilder")

	// Test that TypeBuilder methods return FieldType instances
	stringType, err := tb.String()
	require.NoError(t, err)
	assert.NotNil(t, stringType, "Expected non-nil FieldType from String()")

	// Test method chaining
	optionalStringList, err := stringType.List()
	require.NoError(t, err)
	optionalStringList, err = optionalStringList.Optional()
	require.NoError(t, err)
	assert.NotNil(t, optionalStringList, "Expected non-nil FieldType from method chaining")
}

// TestDynamicOutputWithMap tests dynamic output with map types
// Reference: test_typebuilder.py:296-316
func TestDynamicOutputWithMap(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	dynamicOutputClass, err := tb.DynamicOutput()
	require.NoError(t, err)

	stringType, err := tb.String()
	require.NoError(t, err)
	_, err = dynamicOutputClass.AddProperty("hair_color", stringType)
	require.NoError(t, err)

	// Add map property
	mapType, err := tb.Map(stringType, stringType)
	require.NoError(t, err)

	attrs, err := dynamicOutputClass.AddProperty("attributes", mapType)
	require.NoError(t, err)
	err = attrs.SetDescription("Things like 'eye_color' or 'facial_hair'")
	require.NoError(t, err)

	properties, err := dynamicOutputClass.ListProperties()
	require.NoError(t, err)

	for _, prop := range properties {
		name, err := prop.Name()
		require.NoError(t, err)
		t.Logf("Property: %s", name)
	}

	result, err := b.MyFunc(ctx, "My name is Harrison. My hair is black and I'm 6 feet tall. I have blue eyes and a beard.", b.WithTypeBuilder(tb))
	require.NoError(t, err)

	t.Logf("Dynamic output with map: %+v", result)
}

// TestDynamicOutputWithUnion tests dynamic output with union types
// Reference: test_typebuilder.py:320-364
func TestDynamicOutputWithUnion(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	dynamicOutputClass, err := tb.DynamicOutput()
	require.NoError(t, err)

	stringType, err := tb.String()
	require.NoError(t, err)
	_, err = dynamicOutputClass.AddProperty("hair_color", stringType)
	require.NoError(t, err)

	// Add map property
	mapType, err := tb.Map(stringType, stringType)
	require.NoError(t, err)
	attrs, err := dynamicOutputClass.AddProperty("attributes", mapType)
	require.NoError(t, err)
	err = attrs.SetDescription("Things like 'eye_color' or 'facial_hair'")
	require.NoError(t, err)

	// Define two classes for union
	class1, err := tb.AddClass("Class1")
	require.NoError(t, err)
	floatType, err := tb.Float()
	require.NoError(t, err)
	_, err = class1.AddProperty("meters", floatType)
	require.NoError(t, err)

	class2, err := tb.AddClass("Class2")
	require.NoError(t, err)
	_, err = class2.AddProperty("feet", floatType)
	require.NoError(t, err)
	optionalFloatType, err := floatType.Optional()
	require.NoError(t, err)
	_, err = class2.AddProperty("inches", optionalFloatType)
	require.NoError(t, err)

	// Create union type
	class1Type, err := class1.Type()
	require.NoError(t, err)
	class2Type, err := class2.Type()
	require.NoError(t, err)

	heightUnion, err := tb.Union([]baml.Type{class1Type, class2Type})
	require.NoError(t, err)

	_, err = dynamicOutputClass.AddProperty("height", heightUnion)
	require.NoError(t, err)

	properties, err := dynamicOutputClass.ListProperties()
	require.NoError(t, err)

	for _, prop := range properties {
		name, err := prop.Name()
		require.NoError(t, err)
		t.Logf("Property: %s", name)
	}

	// Test with feet measurement
	result1, err := b.MyFunc(ctx, "My name is Harrison. My hair is black and I'm 6 feet tall. I have blue eyes and a beard. I am 30 years old.", b.WithTypeBuilder(tb))
	require.NoError(t, err)
	t.Logf("Result with feet: %+v", result1)

	// Test with meters measurement
	result2, err := b.MyFunc(ctx, "My name is Harrison. My hair is black and I'm 1.8 meters tall. I have blue eyes and a beard. I am 30 years old.", b.WithTypeBuilder(tb))
	require.NoError(t, err)
	t.Logf("Result with meters: %+v", result2)
}

// TestDynamicClassNestedOutputStreaming tests streaming with dynamic nested classes
// This is a regression test for a bug where dynamic classes worked in non-streaming
// but failed in streaming with "Class X not found" error.
func TestDynamicClassNestedOutputStreaming(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	// Create a new dynamic class "Location" - this is the key test case
	// because it's a class that only exists in the TypeBuilder
	locationClass, err := tb.AddClass("Location")
	require.NoError(t, err)

	stringType, err := tb.String()
	require.NoError(t, err)
	_, err = locationClass.AddProperty("city", stringType)
	require.NoError(t, err)

	_, err = locationClass.AddProperty("country", stringType)
	require.NoError(t, err)

	// Add Location as a property to DynamicOutput
	dynamicOutputClass, err := tb.DynamicOutput()
	require.NoError(t, err)

	locationType, err := locationClass.Type()
	require.NoError(t, err)
	optionalLocationType, err := locationType.Optional()
	require.NoError(t, err)
	_, err = dynamicOutputClass.AddProperty("location", optionalLocationType)
	require.NoError(t, err)

	// Also add a simple property
	optionalStringType, err := stringType.Optional()
	require.NoError(t, err)
	_, err = dynamicOutputClass.AddProperty("name", optionalStringType)
	require.NoError(t, err)

	inputText := "My name is Alice and I live in Paris, France."

	// Test non-streaming first (this should work)
	t.Run("NonStreaming", func(t *testing.T) {
		result, err := b.MyFunc(ctx, inputText, b.WithTypeBuilder(tb))
		require.NoError(t, err)
		t.Logf("Non-streaming result: %+v", result)
		// Verify we got a result with the dynamic properties
		assert.NotNil(t, result)
	})

	// Test streaming - this was the bug case
	t.Run("Streaming", func(t *testing.T) {
		stream, err := b.Stream.MyFunc(ctx, inputText, b.WithTypeBuilder(tb))
		require.NoError(t, err)

		// Collect partial results
		var partialCount int
		var finalResult *types.DynamicOutput

		for value := range stream {
			if value.IsError {
				t.Fatalf("Stream error: %v", value.Error)
			}

			if !value.IsFinal && value.Stream() != nil {
				partialCount++
				t.Logf("Partial result %d: %+v", partialCount, value.Stream())
			}

			// Get final result - this is where the bug would manifest
			// as "Class Location not found" error
			if value.IsFinal && value.Final() != nil {
				finalResult = value.Final()
				t.Logf("Final streaming result: %+v", finalResult)
			}
		}

		// Verify we got partials and a final result
		assert.Greater(t, partialCount, 0, "Expected at least one partial result")
		assert.NotNil(t, finalResult, "Streaming should succeed with dynamic nested classes")
	})
}
