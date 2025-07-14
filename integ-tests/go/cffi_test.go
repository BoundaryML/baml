package main

import (
	"fmt"
	"testing"

	b "example.com/integ-tests/baml_client/types"
	baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
	"github.com/ghetzel/testify/require"
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
		{&b.Recipe{
			Recipe_type: b.Union2KbreakfastOrKdinner__NewKbreakfast(),
			Ingredients: map[string]b.Quantity{
				"a": {
					Amount: b.Union2FloatOrInt__NewInt(1),
				},
			},
		}},
		// {b.RecursiveUnion(*b.Union__string__Map__string_RecursiveUnionNewWithMap__string_RecursiveUnion(&map[string]b.RecursiveUnion{
		// 	"key": b.RecursiveUnion(*b.Union__string__Map__string_RecursiveUnionNewWithString(&[]string{"value"}[0])),
		// 	"key2": b.RecursiveUnion(*b.Union__string__Map__string_RecursiveUnionNewWithMap__string_RecursiveUnion(&map[string]b.RecursiveUnion{
		// 		"key":  b.RecursiveUnion(*b.Union__string__Map__string_RecursiveUnionNewWithString(&[]string{"value2"}[0])),
		// 		"key2": b.RecursiveUnion(*b.Union__string__Map__string_RecursiveUnionNewWithString(&[]string{"value3"}[0])),
		// 	})),
		// }))},
		{map[string]b.Union2BoolOrFloat{
			"a": b.Union2BoolOrFloat__NewBool(true),
			"b": b.Union2BoolOrFloat__NewFloat(2.2),
		}},
	}

	for _, test := range tests {
		t.Run(fmt.Sprintf("EncodeDecode %#v", test.input), func(t *testing.T) {
			encoded, err := baml.BAMLTESTINGONLY_InternalEncode(test.input)
			require.NoError(t, err)

			decoded := baml.Decode(encoded)
			require.Equal(t, test.input, decoded)
		})
	}
}
