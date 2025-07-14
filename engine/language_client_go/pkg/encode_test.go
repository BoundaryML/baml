package baml

import (
	"fmt"
	"testing"

	"github.com/ghetzel/testify/require"
)

func TestEncodeFunctionArguments(t *testing.T) {
	tests := []BamlFunctionArguments{
		{
			Kwargs: map[string]any{"a": "b", "c": 1, "d": 2.2, "e": true},
		},
		{
			Kwargs: map[string]any{"a": "b", "c": 1, "d": 2.2, "e": true},
			ClientRegistry: &ClientRegistry{
				primary: &[]string{"a"}[0],
				clients: clientRegistryMap{
					"a": clientProperty{
						provider: "b",
						options:  map[string]any{"a": "b", "c": 1, "d": 2.2, "e": true},
					},
				},
			},
		},
	}

	for _, test := range tests {
		t.Run(fmt.Sprintf("EncodeFunctionArguments(%v)", test), func(t *testing.T) {
			_, err := EncodeArgs(test)
			require.NoError(t, err)
		})
	}

	t.Run("EncodeMap", func(t *testing.T) {
		test_value := map[string]string{
			"a": "b",
			"c": "d",
			"e": "f",
		}

		encoded_value, err := encodeValue(test_value)
		require.NoError(t, err)

		decoded_value := Decode(encoded_value).Interface()
		require.Equal(t, test_value, decoded_value)
	})

	t.Run("EncodeMapWithOptional", func(t *testing.T) {
		foo, bar := "foo", "bar"
		test_value := map[string]*string{
			"a": &foo,
			"b": &bar,
			"c": nil,
		}

		encoded_value, err := encodeValue(test_value)
		require.NoError(t, err)

		decoded_value := Decode(encoded_value).Interface()
		require.Equal(t, test_value, decoded_value)
	})
}
