package main

import (
	"encoding/json"
	"fmt"
	"reflect"
	"testing"

	b "example.com/integ-tests/baml_client/types"
	"github.com/ghetzel/testify/require"
)

func TestRoundTrip(t *testing.T) {
	var tests = []struct {
		input any
	}{
		{b.Blah{
			Prop4: &[]string{"test"}[0],
		}},
		{b.Union2BoolOrFloat__NewBool(true)},
		{b.Union2BoolOrFloat__NewFloat(1.1)},
	}

	for _, test := range tests {
		t.Run(fmt.Sprintf("RoundTrip %#v", test.input), func(t *testing.T) {
			encoded, err := json.Marshal(test.input)
			require.NoError(t, err)
			emptyOutput := reflect.New(reflect.TypeOf(test.input)).Interface()

			err = json.Unmarshal(encoded, emptyOutput)
			require.NoError(t, err)

			require.Equal(t, test.input, reflect.ValueOf(emptyOutput).Elem().Interface())
		})
	}
}
