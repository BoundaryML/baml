package main

import (
	"context"
	"testing"

	b "example.com/integ-tests/baml_client"
	"github.com/ghetzel/testify/assert"
)

func TestEnvVar(t *testing.T) {
	var tests = map[string]struct {
		envVar string
		envValue string
		err string
	}{
		"required env var": {
			envVar: "OPENAI_API_KEY",
			envValue: "sk-proj-1234567890",
			err: "InvalidAuthentication (401)",
		},
		"not required env var": {
			envVar: "NOT_REQUIRED_ENV_VAR",
			envValue: "",
			err: "",
		},
	}

	for name, test := range tests {
		t.Run(name, func(t *testing.T) {
			t.Setenv(test.envVar, test.envValue)
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

func TestEnvVarWithOptions(t *testing.T) {
	var tests = map[string]struct {
		envVar string
		envValue string
		err string
	}{
		"test override with env var": {
			envVar: "OPENAI_API_KEY",
			envValue: "sk-proj-1234567890",
			err: "InvalidAuthentication (401)",
		},
		"test override with unsetting env var": {
			envVar: "OPENAI_API_KEY",
			envValue: "",
			err: "InvalidAuthentication (401)",
		},
		
	}

	for name, test := range tests {
		t.Run(name, func(t *testing.T) {
			ctx := context.Background()
			_, err := b.AaaSamOutputFormat(ctx, "pineapple", b.WithEnv(map[string]string{
				test.envVar: test.envValue,
			}))
			if test.err != "" {
				assert.Error(t, err)
				assert.Contains(t, err.Error(), test.err)
			} else {
				assert.NoError(t, err)
			}
		})
	}
}
