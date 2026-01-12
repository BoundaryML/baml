// E2E Tests for TypeBuilder and dynamic types
// Mirrors the Rust e2e tests in ../rust/main.rs

package main

import (
	"context"
	"testing"

	b "dynamic_types/baml_client"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestDynamicClassPropertyE2E tests adding property to dynamic class and calling LLM
func TestDynamicClassPropertyE2E(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	personClass, err := tb.Person()
	require.NoError(t, err)

	stringType, err := tb.String()
	require.NoError(t, err)

	// Add dynamic property "occupation" to Person class
	occupationProp, err := personClass.AddProperty("occupation", stringType)
	require.NoError(t, err)
	err = occupationProp.SetDescription("The person's job or profession")
	require.NoError(t, err)

	// Call function with TypeBuilder
	result, err := b.GetPerson(ctx,
		"A software engineer named Alice who is 30 years old and works as a backend developer",
		b.WithTypeBuilder(tb))
	require.NoError(t, err)

	// Verify static fields
	assert.NotEmpty(t, result.Name, "Name should not be empty")
	assert.Greater(t, result.Age, int64(0), "Age should be positive")

	// Verify dynamic field
	occupation, ok := result.DynamicProperties["occupation"]
	assert.True(t, ok, "Person should have dynamic 'occupation' field")
	assert.NotEmpty(t, occupation, "Occupation should not be empty")

	t.Logf("Got person: %s (age %d), occupation: %v", result.Name, result.Age, occupation)
}

// TestMultipleDynamicPropertiesE2E tests adding multiple properties with different types
func TestMultipleDynamicPropertiesE2E(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	personClass, err := tb.Person()
	require.NoError(t, err)

	stringType, err := tb.String()
	require.NoError(t, err)
	boolType, err := tb.Bool()
	require.NoError(t, err)
	intType, err := tb.Int()
	require.NoError(t, err)

	// Add multiple dynamic properties
	emailProp, err := personClass.AddProperty("email", stringType)
	require.NoError(t, err)
	err = emailProp.SetDescription("Email address")
	require.NoError(t, err)

	employedProp, err := personClass.AddProperty("is_employed", boolType)
	require.NoError(t, err)
	err = employedProp.SetDescription("Whether currently employed")
	require.NoError(t, err)

	yearsProp, err := personClass.AddProperty("years_experience", intType)
	require.NoError(t, err)
	err = yearsProp.SetDescription("Years of work experience")
	require.NoError(t, err)

	result, err := b.GetPerson(ctx,
		"Bob Smith, age 35, email bob@example.com, currently employed with 10 years experience",
		b.WithTypeBuilder(tb))
	require.NoError(t, err)

	// Verify static fields
	assert.NotEmpty(t, result.Name)
	assert.Greater(t, result.Age, int64(0))

	// Verify all dynamic fields
	assert.Contains(t, result.DynamicProperties, "email", "Should have email")
	assert.Contains(t, result.DynamicProperties, "is_employed", "Should have is_employed")
	assert.Contains(t, result.DynamicProperties, "years_experience", "Should have years_experience")

	t.Logf("Person: %s, email: %v, employed: %v, years: %v",
		result.Name,
		result.DynamicProperties["email"],
		result.DynamicProperties["is_employed"],
		result.DynamicProperties["years_experience"])
}

// TestDynamicEnumValueE2E tests adding enum values and classifying correctly
func TestDynamicEnumValueE2E(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	categoryEnum, err := tb.Category()
	require.NoError(t, err)

	// Add new enum values to Category
	sportsVal, err := categoryEnum.AddValue("Sports")
	require.NoError(t, err)
	err = sportsVal.SetDescription("Sports and athletics news")
	require.NoError(t, err)

	politicsVal, err := categoryEnum.AddValue("Politics")
	require.NoError(t, err)
	err = politicsVal.SetDescription("Political news and government")
	require.NoError(t, err)

	entertainmentVal, err := categoryEnum.AddValue("Entertainment")
	require.NoError(t, err)
	err = entertainmentVal.SetDescription("Movies, TV, celebrities")
	require.NoError(t, err)

	result, err := b.ClassifyArticle(ctx,
		"The Lakers won the championship last night with a stunning 3-pointer in overtime",
		b.WithTypeBuilder(tb))
	require.NoError(t, err)

	categoryStr := string(result)
	t.Logf("Category: %s", categoryStr)

	// Should be one of our categories
	validCategories := []string{"Sports", "Technology", "Science", "Arts", "Politics", "Entertainment"}
	assert.Contains(t, validCategories, categoryStr, "Should be a valid category")
}

// TestNestedDynamicTypesE2E tests handling nested dynamic types
func TestNestedDynamicTypesE2E(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	stringType, err := tb.String()
	require.NoError(t, err)
	intType, err := tb.Int()
	require.NoError(t, err)
	boolType, err := tb.Bool()
	require.NoError(t, err)

	// Add dynamic property to Person (nested in Article)
	personClass, err := tb.Person()
	require.NoError(t, err)
	_, err = personClass.AddProperty("bio", stringType)
	require.NoError(t, err)

	// Add dynamic properties to Article
	articleClass, err := tb.Article()
	require.NoError(t, err)
	_, err = articleClass.AddProperty("word_count", intType)
	require.NoError(t, err)
	_, err = articleClass.AddProperty("published", boolType)
	require.NoError(t, err)

	// Add new category value
	categoryEnum, err := tb.Category()
	require.NoError(t, err)
	_, err = categoryEnum.AddValue("Business")
	require.NoError(t, err)

	result, err := b.CreateArticle(ctx,
		"A 500-word published article about tech startups by John Doe, a tech journalist",
		b.WithTypeBuilder(tb))
	require.NoError(t, err)

	// Verify static fields
	assert.NotEmpty(t, result.Title, "Title should not be empty")
	assert.NotEmpty(t, result.Author.Name, "Author name should exist")

	// Verify dynamic fields on Article
	assert.Contains(t, result.DynamicProperties, "word_count", "Article should have word_count")
	assert.Contains(t, result.DynamicProperties, "published", "Article should have published")

	// Verify dynamic field on nested Person
	assert.Contains(t, result.Author.DynamicProperties, "bio", "Author should have bio")

	t.Logf("Article: %s by %s (category: %s)", result.Title, result.Author.Name, result.Category)
}

// TestComplexDynamicTypesE2E tests handling lists and optionals in dynamic properties
func TestComplexDynamicTypesE2E(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	stringType, err := tb.String()
	require.NoError(t, err)

	// Add list of strings
	listType, err := stringType.List()
	require.NoError(t, err)

	personClass, err := tb.Person()
	require.NoError(t, err)
	_, err = personClass.AddProperty("skills", listType)
	require.NoError(t, err)

	// Add optional string
	optionalType, err := stringType.Optional()
	require.NoError(t, err)
	nicknameProp, err := personClass.AddProperty("nickname", optionalType)
	require.NoError(t, err)
	err = nicknameProp.SetDescription("The person's nickname (if explicitly provided)")
	require.NoError(t, err)

	heightType, err := tb.Float()
	require.NoError(t, err)
	heightProp, err := personClass.AddProperty("height", heightType)
	require.NoError(t, err)
	err = heightProp.SetDescription("The person's height in meters")
	require.NoError(t, err)

	result, err := b.GetPerson(ctx,
		"Alice Johnson, 28, skills: Rust, Python, Go. Nickname: AJ. Height: 1.8 meters.",
		b.WithTypeBuilder(tb))
	require.NoError(t, err)

	t.Logf("Person: %s (age %d)", result.Name, result.Age)

	// Check skills list
	skills, ok := result.DynamicProperties["skills"]
	assert.True(t, ok, "Person should have skills")
	skillsValue, ok := skills.([]string)
	assert.True(t, ok, "Skills should be a list")
	assert.Len(t, skillsValue, 3, "Skills should have 3 items")

	// Check height
	height, ok := result.DynamicProperties["height"]
	assert.True(t, ok, "Person should have height")
	heightValue, ok := height.(float64)
	assert.True(t, ok, "Height should be a float64")
	assert.Equal(t, 1.8, heightValue, "Height should be 1.8 meters")

	// Check optional nickname
	nickname, ok := result.DynamicProperties["nickname"]
	assert.True(t, ok, "Person should have nickname")
	nicknameValue, ok := nickname.(*string)
	assert.True(t, ok, "Nickname should be an optional string")
	assert.Equal(t, "AJ", *nicknameValue, "Nickname should be AJ")
}

// TestAliasE2E tests using alias for better LLM matching
func TestAliasE2E(t *testing.T) {
	ctx := context.Background()

	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	categoryEnum, err := tb.Category()
	require.NoError(t, err)

	// Add a category with an alias
	aiVal, err := categoryEnum.AddValue("AI")
	require.NoError(t, err)
	err = aiVal.SetAlias("Artificial Intelligence")
	require.NoError(t, err)
	err = aiVal.SetDescription("Artificial intelligence and machine learning")
	require.NoError(t, err)

	result, err := b.ClassifyArticle(ctx,
		"GPT-5 achieves human-level reasoning in new benchmarks, researchers claim",
		b.WithTypeBuilder(tb))
	require.NoError(t, err)

	categoryStr := string(result)
	t.Logf("Category for AI article: %s", categoryStr)

	// Should be AI or Technology
	assert.Contains(t, []string{"AI", "Technology"}, categoryStr, "Expected AI-related category")
}

// TestFullyDynamicClassE2E tests creating completely new class at runtime
func TestFullyDynamicClassE2E(t *testing.T) {
	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)

	stringType, err := tb.String()
	require.NoError(t, err)
	floatType, err := tb.Float()
	require.NoError(t, err)
	boolType, err := tb.Bool()
	require.NoError(t, err)

	// Create a completely new class at runtime
	productClass, err := tb.AddClass("Product")
	require.NoError(t, err)
	_, err = productClass.AddProperty("name", stringType)
	require.NoError(t, err)
	_, err = productClass.AddProperty("price", floatType)
	require.NoError(t, err)
	_, err = productClass.AddProperty("in_stock", boolType)
	require.NoError(t, err)

	// Verify the type is registered
	productType, err := productClass.Type()
	require.NoError(t, err)
	t.Logf("Created dynamic Product type")

	// Note: Can't call a function that returns Product directly
	// because it's not in the schema, but we verify the type exists
	assert.NotNil(t, productType)
}
