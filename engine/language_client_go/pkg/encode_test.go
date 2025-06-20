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
			_, err := EncodeRoot(test)
			require.NoError(t, err)
		})
	}
}
