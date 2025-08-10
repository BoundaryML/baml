package main

import (
	"fmt"
	"testing"

	b "example.com/integ-tests/baml_client"
	"example.com/integ-tests/baml_client/stream_types"
	"example.com/integ-tests/baml_client/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestParseLLMResponse tests parsing LLM responses
// Reference: test_parser.py:7-43
func TestParseLLMResponse(t *testing.T) {
	llmResponse := `
        ` + "```json" + `
        {
            "len": 5,
            "head": {
                "data": 1,
                "next": {
                    "data": 2,
                    "next": {
                        "data": 3,
                        "next": {
                            "data": 4,
                            "next": {
                                "data": 5,
                                "next": null
                            }
                        }
                    }
                }
            }
        }
        ` + "```" + `
    `

	parsed, err := b.Parse.BuildLinkedList(llmResponse)
	require.NoError(t, err)

	expected := types.LinkedList{
		Len: 5,
		Head: &types.Node{
			Data: 1,
			Next: &types.Node{
				Data: 2,
				Next: &types.Node{
					Data: 3,
					Next: &types.Node{
						Data: 4,
						Next: &types.Node{
							Data: 5,
							Next: nil,
						},
					},
				},
			},
		},
	}

	assert.Equal(t, expected, parsed)
}

// TestParseLLMResponseSync tests synchronous parsing
// Reference: test_parser.py:46-82
func TestParseLLMResponseSync(t *testing.T) {
	llmResponse := `
        ` + "```json" + `
        {
            "len": 5,
            "head": {
                "data": 1,
                "next": {
                    "data": 2,
                    "next": {
                        "data": 3,
                        "next": {
                            "data": 4,
                            "next": {
                                "data": 5,
                                "next": null
                            }
                        }
                    }
                }
            }
        }
        ` + "```" + `
    `

	parsed, err := b.Parse.BuildLinkedList(llmResponse)
	require.NoError(t, err)

	expected := types.LinkedList{
		Len: 5,
		Head: &types.Node{
			Data: 1,
			Next: &types.Node{
				Data: 2,
				Next: &types.Node{
					Data: 3,
					Next: &types.Node{
						Data: 4,
						Next: &types.Node{
							Data: 5,
							Next: nil,
						},
					},
				},
			},
		},
	}

	assert.Equal(t, expected, parsed)
}

// TestParseLLMStream tests parsing streaming LLM responses
// Reference: test_parser.py:85-124
func TestParseLLMStream(t *testing.T) {
	stream := `
        ` + "```json" + `
        {
            "name": "John Doe",
            "email": "john.doe@example.com",
        ` + "```" + `
    `

	parsed, err := b.ParseStream.ExtractResume(stream)
	require.NoError(t, err)

	expected := stream_types.Resume{
		Name:       stringPtr("John Doe"),
		Email:      stringPtr("john.doe@example.com"),
		Phone:      nil,
		Experience: []string{},
		Education:  []stream_types.Education{},
		Skills:     []string{},
	}

	assert.Equal(t, expected, parsed)
}

// TestParseJSONExtraction tests extracting JSON from text
func TestParseJSONExtraction(t *testing.T) {
	// Test various JSON formats within text
	testCases := []struct {
		name     string
		input    string
		expected bool // whether parsing should succeed
	}{
		{
			name: "SimpleJSON",
			input: `{"name": "John", "age": 30}`,
			expected: true,
		},
		{
			name: "JSONWithCodeBlocks",
			input: "```json\n{\"name\": \"John\", \"age\": 30}\n```",
			expected: true,
		},
		{
			name: "JSONInText",
			input: "Here is the data: {\"name\": \"John\", \"age\": 30} and more text",
			expected: true,
		},
		{
			name: "NoJSON",
			input: "This is just plain text with no JSON",
			expected: true,
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			// Test with a simple parsing function
			_, err := b.Parse.JsonTypeAliasCycle(tc.input)
			
			if tc.expected {
				assert.NoError(t, err, "Expected successful parsing for %s", tc.name)
			} else {
				assert.Error(t, err, "Expected parsing error for %s", tc.name)
			}
		})
	}
}

// TestParseComplexStructures tests parsing complex nested structures
func TestParseComplexStructures(t *testing.T) {
	complexJSON := `{
		"name": "John Doe",
		"email": "john@example.com",
		"phone": "123-456-7890",
		"experience": [
			"Software Engineer at Google (2020-Present)",
			"Developer at Microsoft (2018-2020)"
		],
		"education": [
			{
				"institution": "MIT",
				"location": "Cambridge, MA",
				"degree": "Bachelor of Science",
				"major": ["Computer Science"],
				"graduation_date": "2018"
			}
		],
		"skills": ["Go", "Python", "JavaScript"]
	}`

	parsed, err := b.Parse.ExtractResume(complexJSON)
	require.NoError(t, err)

	assert.NotNil(t, parsed.Name)
	assert.Equal(t, "John Doe", parsed.Name)
	assert.NotNil(t, parsed.Email)
	assert.Equal(t, "john@example.com", parsed.Email)
	assert.NotNil(t, parsed.Phone)
	assert.Equal(t, "123-456-7890", parsed.Phone)
	assert.Len(t, parsed.Experience, 2)
	assert.Len(t, parsed.Education, 1)
	assert.Len(t, parsed.Skills, 3)
	
	// Check education details
	education := parsed.Education[0]
	assert.Equal(t, "MIT", education.Institution)
	assert.Equal(t, "Cambridge, MA", education.Location)
	assert.Equal(t, "Bachelor of Science", education.Degree)
	assert.Contains(t, education.Major, "Computer Science")
}

// TestParseErrorHandling tests error handling in parsing
func TestParseErrorHandling(t *testing.T) {
	testCases := []struct {
		name  string
		input string
	}{
		{
			name:  "InvalidJSON",
			input: `{"name": "John", "age":}`, // Invalid JSON
		},
		{
			name:  "WrongStructure",
			input: `{"wrong": "structure"}`, // Valid JSON, wrong structure
		},
		{
			name:  "EmptyString",
			input: ``,
		},
		{
			name:  "NonJSONText",
			input: `This is just plain text`,
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			_, err := b.Parse.ExtractResume(tc.input)
			assert.Error(t, err, "Expected parsing error for %s", tc.name)
		})
	}
}

// TestParsePartialStreaming tests parsing partial streaming responses
func TestParsePartialStreaming(t *testing.T) {
	// Test incomplete JSON that might come from streaming
	partialResponses := []string{
		`{"name":`,
		`{"name": "John"`,
		`{"name": "John", "email":`,
		`{"name": "John", "email": "john@example.com"`,
		`{"name": "John", "email": "john@example.com", "phone": "123-456-7890"}`,
	}

	for i, partial := range partialResponses {
		t.Run(fmt.Sprintf("Partial%d", i), func(t *testing.T) {
			parsed, err := b.ParseStream.ExtractResume(partial)
			
			if err != nil {
				// Some partial responses might fail, which is expected
				t.Logf("Partial response %d failed as expected: %v", i, err)
				return
			}

			// If parsing succeeds, verify the available fields
			if parsed.Name != nil {
				assert.Equal(t, "John", *parsed.Name)
			}
			if parsed.Email != nil {
				assert.Equal(t, "john@example.com", *parsed.Email)
			}
			if parsed.Phone != nil {
				assert.Equal(t, "123-456-7890", *parsed.Phone)
			}
		})
	}
}

// TestParseWithDifferentFormats tests parsing different response formats
func TestParseWithDifferentFormats(t *testing.T) {
	formats := []struct {
		name   string
		format string
	}{
		{
			name:   "PlainJSON",
			format: `{"prop1": "value1", "prop2": 42}`,
		},
		{
			name:   "JSONWithBackticks",
			format: "```json\n{\"prop1\": \"value1\", \"prop2\": 42}\n```",
		},
		{
			name:   "JSONWithLanguageTag",
			format: "```javascript\n{\"prop1\": \"value1\", \"prop2\": 42}\n```",
		},
		{
			name:   "JSONInSentence",
			format: "The result is {\"prop1\": \"value1\", \"prop2\": 42} which looks good.",
		},
	}

	for _, format := range formats {
		t.Run(format.name, func(t *testing.T) {
			parsed, err := b.Parse.FnOutputClass(format.format)
			require.NoError(t, err, "Expected successful parsing for format: %s", format.name)
			assert.NotEmpty(t, parsed.Prop1, "Expected prop1 to be parsed")
			assert.Equal(t, int64(42), parsed.Prop2, "Expected prop2 to be 42")
		})
	}
}

// TestParseUnionTypes tests parsing union type responses
func TestParseUnionTypes(t *testing.T) {
	// Test parsing responses that could be multiple types
	testCases := []struct {
		name     string
		input    string
		expected interface{}
	}{
		{
			name:  "LiteralInt",
			input: "1",
		},
		{
			name:  "LiteralBool",
			input: "true",
		},
		{
			name:  "LiteralString",
			input: `"string output"`,
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			// Use a function that returns union types
			result, err := b.Parse.LiteralUnionsTest(tc.input)
			require.NoError(t, err, "Expected successful parsing for %s", tc.name)
			assert.NotNil(t, result, "Expected non-nil result")
		})
	}
}
