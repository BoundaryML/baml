package baml

import (
	"fmt"
	"testing"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/serde"
	"github.com/ghetzel/testify/require"
)

func TestEncodeFunctionArguments(t *testing.T) {
	client := NewClientRegistry()
	client.AddLlmClient("a", "b", map[string]any{"a": "b", "c": 1, "d": 2.2, "e": true})
	client.SetPrimaryClient("a")

	tests := []BamlFunctionArguments{
		{
			Kwargs: map[string]any{"a": "b", "c": 1, "d": 2.2, "e": true},
		},
		{
			Kwargs:         map[string]any{"a": "b", "c": 1, "d": 2.2, "e": true},
			ClientRegistry: client,
		},
	}

	for _, test := range tests {
		t.Run(fmt.Sprintf("EncodeFunctionArguments(%v)", test), func(t *testing.T) {
			_, err := test.Encode()
			require.NoError(t, err)
		})
	}

	t.Run("EncodeMap", func(t *testing.T) {
		test_value := map[string]string{
			"a": "b",
			"c": "d",
			"e": "f",
		}

		_, err := serde.EncodeValue(test_value)
		require.NoError(t, err)
	})

	t.Run("EncodeMapWithOptional", func(t *testing.T) {
		foo, bar := "foo", "bar"
		test_value := map[string]*string{
			"a": &foo,
			"b": &bar,
			"c": nil,
		}

		_, err := serde.EncodeValue(test_value)
		require.NoError(t, err)
	})
}
