// Reference implementation for the text-stats task in Go.
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"strings"
	"unicode/utf8"
)

func main() {
	if len(os.Args) != 2 {
		fmt.Fprintln(os.Stderr, "usage: text_stats <path>")
		os.Exit(2)
	}
	raw, err := os.ReadFile(os.Args[1])
	if err != nil {
		fmt.Fprintf(os.Stderr, "read: %v\n", err)
		os.Exit(2)
	}
	text := string(raw)
	out := map[string]int{
		"bytes": len(raw),
		"chars": utf8.RuneCountInString(text),
		"words": len(strings.Fields(text)),
		"lines": strings.Count(text, "\n"),
	}
	enc, _ := json.Marshal(out)
	fmt.Println(string(enc))
}
