package main

import (
	b "classes/baml_client"
	"context"
	"encoding/json"
	"fmt"

	"classes/baml_client/types"
)

func main() {
	cls := types.SimpleClass{}
	cls.Digits = 10
	cls.Words = "hello"

	result, err := b.ConsumeSimpleClass(context.Background(), cls)
	if err != nil {
		panic(err)
	}
	fmt.Println(result)

	channel, err := b.Stream.MakeSimpleClass(context.Background())
	if err != nil {
		panic(err)
	}
	for result := range channel {
		if result.IsFinal {
			fmt.Println("final-----")
			str, err := json.Marshal(result.Final())
			if err != nil {
				panic(err)
			}
			fmt.Println(string(str))
		} else {
			fmt.Println(result.Stream().Words.State)
			fmt.Println(result.Stream().Words.Value)
		}
	}
}
