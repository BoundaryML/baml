package main

import "encoding/json"

type Hello struct {
	Message string `json:"message"`
}

func Setup() *Hello {
	return &Hello{Message: "Hello, World!"}
}

func Run(hello *Hello) string {
	out, err := json.Marshal(hello)
	if err != nil {
		panic(err)
	}
	return string(out)
}

func Main() string {
	return Run(Setup())
}
