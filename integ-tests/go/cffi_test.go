package main

import (
	"fmt"
	"testing"

	b "example.com/integ-tests/baml_client/types"
	baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
	"github.com/ghetzel/testify/require"
	flatbuffers "github.com/google/flatbuffers/go"
)

func TestEncodeDecode(t *testing.T) {
	var tests = []struct {
		input any
	}{
		{map[string]int64{"a": 1, "b": 2}},
		{map[string]float64{"a": 1.1, "b": 2.2}},
		{map[string]bool{"a": true, "b": false}},
		{map[string]string{"a": "hello", "b": "world"}},
		{[]int64{1, 2, 3}},
		{[]float64{1.1, 2.2, 3.3}},
		{[]bool{true, false, true}},
		{[]string{"hello", "world"}},
		{&b.BlockConstraint{
			Foo: 1,
			Bar: "bar",
		}},
		{map[string]b.BlockConstraint{
			"a": {
				Foo: 1,
				Bar: "bar",
			},
			"b": {
				Foo: 2,
				Bar: "baz",
			},
		}},
		{[]b.BlockConstraint{
			{
				Foo: 1,
				Bar: "bar",
			},
			{
				Foo: 2,
				Bar: "baz",
			},
		}},
		{&b.Education{
			Institution:     "MIT",
			Location:        "Cambridge, MA",
			Degree:          "Bachelor of Science",
			Major:           []string{"Computer Science", "Mathematics"},
			Graduation_date: &[]string{"2024"}[0],
		}},
		{&b.Education{
			Institution: "MIT",
			Location:    "Cambridge, MA",
			Degree:      "Bachelor of Science",
			Major:       []string{"Computer Science", "Mathematics"},
		}},
		{&b.Person{
			Name:       &[]string{"John Doe"}[0],
			Hair_color: &[]b.Color{b.ColorRED}[0],
		}},
		// TODO: fix this
		// {map[string]b.Union__float__bool{
		// 	"a": *b.Union__float__boolNewWithBool(&[]bool{true}[0]),
		// 	"b": *b.Union__float__boolNewWithFloat(&[]float64{2.2}[0]),
		// }},
	}

	for _, test := range tests {
		t.Run(fmt.Sprintf("EncodeDecode %#v", test.input), func(t *testing.T) {
			encoded, err := baml.EncodeRoot(test.input)
			require.NoError(t, err)

			holder := cffi.CFFIValueHolder{}
			flatbuffers.GetRootAs(encoded, 0, &holder)
			decoded := baml.Decode(&holder)

			require.Equal(t, test.input, decoded)
		})
	}
}
