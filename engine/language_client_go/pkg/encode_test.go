package baml

import (
	"testing"

	"github.com/ghetzel/testify/require"
)

func TestEncodeFunctionArguments(t *testing.T) {
	args := BamlFunctionArguments{Kwargs: map[string]any{"a": "b", "c": 1, "d": 2.2, "e": true}}
	_, err := EncodeRoot(args)
	require.NoError(t, err)
}
