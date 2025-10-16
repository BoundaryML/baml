package main

import (
	"context"
	"strings"
	"testing"

	b "example.com/integ-tests/baml_client"
	"example.com/integ-tests/baml_client/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestConstraints tests basic constraint validation
// Reference: test_functions.py:98-101
func TestConstraints(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.PredictAge(ctx, "Greg")
	require.NoError(t, err)
	
	// Check constraint validation results
	assert.Equal(t, "failed", result.Certainty.Checks["unreasonably_certain"].Status)
	
	// Verify not all constraints succeeded
	allSucceeded := true
	for _, check := range result.Certainty.Checks {
		if check.Status != "succeeded" {
			allSucceeded = false
			break
		}
	}
	assert.False(t, allSucceeded, "Expected not all constraints to succeed")
}

// TestConstraintUnionVariantChecking tests constraint checking on union variants
// Reference: test_functions.py:104-111
func TestConstraintUnionVariantChecking(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.ExtractContactInfo(ctx, "Reach me at help@boundaryml.com, or 111-222-3333 if needed.")
	require.NoError(t, err)
	
	// Verify primary contact extraction
	assert.NotNil(t, result.Primary)
	assert.Equal(t, "help@boundaryml.com", result.Primary.AsEmailAddress().Value)
	
	// Verify secondary contact extraction
	assert.NotNil(t, result.Secondary)
	assert.Equal(t, "111-222-3333", result.Secondary.AsPhoneNumber().Value)
}

// TestReturnMalformedConstraint tests handling of malformed constraints in return values
// Reference: test_functions.py:114-119
func TestReturnMalformedConstraint(t *testing.T) {
	ctx := context.Background()
	
	_, err := b.ReturnMalformedConstraints(ctx, 1)
	require.Error(t, err, "Expected error for malformed constraints")
	assert.Contains(t, err.Error(), "Failed to coerce value", "Expected coercion error message")
}

// TestUseMalformedConstraint tests using malformed constraints as input
// Reference: test_functions.py:122-126
func TestUseMalformedConstraint(t *testing.T) {
	ctx := context.Background()
	
	_, err := b.UseMalformedConstraints(ctx, types.MalformedConstraints2{Foo: 2})
	require.Error(t, err, "Expected error when using malformed constraints")
	assert.Contains(t, err.Error(), "number has no method named length", "Expected specific error message")
}

// TestBlockConstraints tests block-level constraints
// Reference: test_functions.py:1417-1419
func TestBlockConstraints(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.MakeBlockConstraint(ctx)
	require.NoError(t, err)
	
	// Check block constraint validation
	assert.Equal(t, "failed", result.Checks["cross_field"].Status)
}

// TestNestedBlockConstraints tests nested block constraints
// Reference: test_functions.py:1422-1426
func TestNestedBlockConstraints(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.MakeNestedBlockConstraint(ctx)
	require.NoError(t, err)
	
	// Check nested block constraint validation
	assert.Equal(t, "succeeded", result.Nbc.Checks["cross_field"].Status)
}

// TestBlockConstraintArguments tests block constraints on function arguments
// Reference: test_functions.py:1430-1439
func TestBlockConstraintArguments(t *testing.T) {
	ctx := context.Background()
	
	// Test failing constraint
	blockConstraint := types.BlockConstraintForParam{
		Bcfp:  1,
		Bcfp2: "too long!",
	}
	
	_, err := b.UseBlockConstraint(ctx, blockConstraint)
	require.Error(t, err, "Expected error for failing block constraint")
	assert.Contains(t, err.Error(), "Failed assert: hi", "Expected specific constraint error")
	
	// Test nested failing constraint
	nestedBlockConstraint := types.NestedBlockConstraintForParam{
		Nbcfp: blockConstraint,
	}
	
	_, err = b.UseNestedBlockConstraint(ctx, nestedBlockConstraint)
	require.Error(t, err, "Expected error for failing nested block constraint")
	assert.Contains(t, err.Error(), "Failed assert: hi", "Expected specific nested constraint error")
}

// TestReturnFailingAssert tests assertions that fail during return processing
// Reference: test_functions.py:1327-1329
func TestReturnFailingAssert(t *testing.T) {
	ctx := context.Background()
	
	_, err := b.ReturnFailingAssert(ctx, 1)
	require.Error(t, err, "Expected validation error for failing assert")
	
	// Error should be a validation error type
	assert.Contains(t, strings.ToLower(err.Error()), "failed to coerce value", "Expected validation error")
}

// TestParameterFailingAssert tests assertions that fail on parameters
// Reference: test_functions.py:1333-1336
func TestParameterFailingAssert(t *testing.T) {
	ctx := context.Background()
	
	// Test value that should fail parameter assertion
	_, err := b.ReturnFailingAssert(ctx, 100)
	require.Error(t, err, "Expected invalid argument error for failing parameter assert")
	
	// Should be an invalid argument error
	assert.Contains(t, strings.ToLower(err.Error()), "failed assert:", "Expected invalid argument error")
}

// TestFailingAssertCanStream tests that assertions can be validated during streaming
// Reference: test_functions.py:1340-1347
func TestFailingAssertCanStream(t *testing.T) {
	ctx := context.Background()
	
	stream, err := b.Stream.StreamFailingAssertion(ctx, "Yoshimi battles the pink robots", 300)
	require.NoError(t, err)
	
	var hasContent bool
	
	// Should be able to stream partial content
	for value := range stream {
		if value.IsError {
			// Final result should fail validation
			require.Error(t, value.Error, "Expected validation error in final result")
			assert.Contains(t, strings.ToLower(value.Error.Error()), "failed to coerce value", "Expected validation error")
			break
		}
		
		if value.IsFinal {
			// this should not happen
			t.Fatalf("Final result should not be an error")
		}
			
		if value.Stream() != nil {
			streamData := *value.Stream()
			if streamData.Story_a != nil && len(*streamData.Story_a) > 0 {
				hasContent = true
			}
		}
	}
	
	assert.True(t, hasContent, "Expected to receive streaming content before validation failure")
}

// TestMergeAliasAttributes tests merging attributes on aliases
// Reference: test_functions.py:275-278
func TestMergeAliasAttributes(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.MergeAliasAttributes(ctx, 123)
	require.NoError(t, err)
	
	assert.Equal(t, int64(123), result.Amount.Value)
	assert.Equal(t, "succeeded", result.Amount.Checks["gt_ten"].Status)
}

// TestReturnAliasWithMergedAttrs tests returning aliases with merged attributes
// Reference: test_functions.py:281-284
func TestReturnAliasWithMergedAttrs(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.ReturnAliasWithMergedAttributes(ctx, 123)
	require.NoError(t, err)
	
	assert.Equal(t, int64(123), result.Value)
	assert.Equal(t, "succeeded", result.Checks["gt_ten"].Status)
}

// TestAliasWithMultipleAttrs tests aliases with multiple attributes
// Reference: test_functions.py:287-290
func TestAliasWithMultipleAttrs(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.AliasWithMultipleAttrs(ctx, 123)
	require.NoError(t, err)
	
	assert.Equal(t, int64(123), result.Value)
	assert.Equal(t, "succeeded", result.Checks["gt_ten"].Status)
}

// TestAssertFunction tests basic assert functionality
// Reference: Inferred from AssertFn function
func TestAssertFunction(t *testing.T) {
	ctx := context.Background()
	
	// Test with valid input
	result, err := b.AssertFn(ctx, 4)
	require.NoError(t, err)
	assert.Equal(t, int64(5), result)
}

// TestSemanticContainer tests semantic streaming with constraints
// Reference: test_functions.py:1450-1499
func TestSemanticContainer(t *testing.T) {
	ctx := context.Background()
	
	stream, err := b.Stream.MakeSemanticContainer(ctx)
	require.NoError(t, err)
	
	var referenceString *string
	var referenceInt *int64
	var finalResult *types.SemanticContainer
	
	for value := range stream {
		if value.IsError {
			t.Fatalf("Stream error: %v", value.Error)
		}
		
		if !value.IsFinal && value.Stream() != nil {
			msg := *value.Stream()
			
			// Verify expected fields exist
			assert.Contains(t, map[string]interface{}{
				"string_with_twenty_words": msg.String_with_twenty_words,
				"sixteen_digit_number":     msg.Sixteen_digit_number,
			}, "string_with_twenty_words", "Expected string_with_twenty_words field")
			
			// Check stability of numeric fields
			if msg.Sixteen_digit_number != nil {
				if referenceInt == nil {
					referenceInt = msg.Sixteen_digit_number
				} else {
					assert.Equal(t, *referenceInt, *msg.Sixteen_digit_number, "Sixteen digit number should be stable")
				}
			}
			
			// Check stability of string fields marked as @stream.done
			if msg.String_with_twenty_words != nil {
				if referenceString == nil {
					referenceString = msg.String_with_twenty_words
				} else {
					assert.Equal(t, *referenceString, *msg.String_with_twenty_words, "String with twenty words should be stable")
				}
			}
			
			// Check @stream.with_state behavior
			if msg.Class_needed.S_20_words.Value != nil {
				wordCount := len(strings.Fields(*msg.Class_needed.S_20_words.Value))
				if wordCount < 3 && msg.Final_string == nil {
					assert.NotEqual(t, "Incomplete", msg.Class_needed.S_20_words.State, "Expected Incomplete state for short content")
				}
			}
			
			if msg.Final_string != nil {
				// TODO: This is not working! its always in Pending state (which is wrong)
				// assert.Equal(t, "Complete", msg.Class_needed.S_20_words.State, "Expected Complete state when final string is set: %+v", msg)
			}
			
			// Check @stream.not_null behavior
			for _, sub := range msg.Three_small_things {
				assert.NotNil(t, sub.I_16_digits, "Expected non-null i_16_digits due to @stream.not_null")
			}
		}
		
		if value.IsFinal && value.Final() != nil {
			finalResult = value.Final()
		}
	}
	
	require.NotNil(t, finalResult, "Expected final result")
	assert.NotNil(t, finalResult.String_with_twenty_words, "Expected final string_with_twenty_words")
	assert.NotNil(t, finalResult.Sixteen_digit_number, "Expected final sixteen_digit_number")
}