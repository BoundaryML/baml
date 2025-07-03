package main

import (
	"context"
	"encoding/json"
	"fmt"
	b "unions/baml_client"
	types "unions/baml_client/types"
)

func main() {
	// result, err := b.Foo(context.Background(), 8192)
	// if err != nil {
	// 	panic(err)
	// }
	// fmt.Println(result)

	fmt.Println("hello", b.Stream)

	category := types.Union2KresourceOrKservice__NewKservice()
	input := types.ExistingSystemComponent{
		Id:          1,
		Name:        "Hello",
		Type:        "service",
		Category:    category,
		Explanation: "Hello",
	}
	array := []types.ExistingSystemComponent{input}

	stream, err := b.Stream.JsonInput(context.Background(), array)
	if err != nil {
		panic(err)
	}
	for msg := range stream {
		if msg.IsFinal {
			fmt.Println("final")
			json_bytes, err := json.Marshal(msg.Final())
			if err != nil {
				panic(err)
			}
			fmt.Println(string(json_bytes))
		} else {
			fmt.Println("stream")
			json_bytes, err := json.Marshal(msg.Stream())
			if err != nil {
				panic(err)
			}
			fmt.Println(string(json_bytes))
		}
	}

	// content_bytes, err := baml.EncodeRoot(array)
	// if err != nil {
	// 	panic(err)
	// }

	// parsed_data := cffi.CFFIValueHolder{}
	// decoded_data := baml.Decode(&parsed_data)

	// fmt.Println(decoded_data)

	// // Ensure they are the same
	// if !reflect.DeepEqual(array, decoded_data) {
	// 	// Print the JSON diff
	// 	json1, err := json.Marshal(array)
	// 	if err != nil {
	// 		panic(err)
	// 	}
	// 	json2, err := json.Marshal(decoded_data)
	// 	if err != nil {
	// 		panic(err)
	// 	}
	// 	fmt.Println(string(json1))
	// 	fmt.Println(string(json2))
	// 	panic("arrays are not the same")
	// }

	// result, err := b.JsonInput(context.Background(), array)
	// if err != nil {
	// 	panic(err)
	// }
	// fmt.Println(result)
}
