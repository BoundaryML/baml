package main

import (
	"context"
	"os"
	"testing"

	b "example.com/integ-tests/baml_client"
	"github.com/ghetzel/testify/assert"
)

func TestEnvVar(t *testing.T) {
	var tests = map[string]struct {
		envVar string
		expected string
		err string
	}{
		"OPENAI_API_KEY": {
			envVar: "OPENAI_API_KEY",
			expected: "sk-proj-1234567890",
			err: "InvalidAuthentication (401)",
		},
		"NOT_REQUIRED_ENV_VAR": {
			envVar: "NOT_REQUIRED_ENV_VAR",
			expected: "",
			err: "",
		},
	}

	for name, test := range tests {
		t.Run(name, func(t *testing.T) {
			t.Setenv(test.envVar, test.expected)
			assert.Equal(t, test.expected, os.Getenv(test.envVar))
			ctx := context.Background()
			_, err := b.AaaSamOutputFormat(ctx, "pineapple")
			if test.err != "" {
				assert.Error(t, err)
				assert.Contains(t, err.Error(), test.err)
			} else {
				assert.NoError(t, err)
			}
		})
	}
}
