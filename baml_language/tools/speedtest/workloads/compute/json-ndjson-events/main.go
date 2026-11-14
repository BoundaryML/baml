package main

import (
	"encoding/json"
	"strconv"
	"strings"
)

type Event struct {
	ID        string  `json:"id"`
	SessionID string  `json:"session_id"`
	EventType string  `json:"event_type"`
	Role      *string `json:"role"`
	Content   *string `json:"content"`
	ToolName  *string `json:"tool_name"`
	CreatedAt int     `json:"created_at"`
}

type Rng struct {
	seed int
}

func draw(rng *Rng, n int) int {
	rng.seed = rng.seed * 16807 % 2147483647
	return rng.seed % n
}

func Setup() []string {
	rng := &Rng{seed: 23}
	words := []string{"agent", "tool", "result", "latency", "token", "stream", "retry", "model", "prompt", "cache", "session", "vector"}
	types := []string{"message", "tool_call", "tool_result", "status"}
	roles := []string{"user", "assistant", "system"}
	tools := []string{"search", "calculator", "browser", "shell"}
	lines := []string{}
	for i := 0; i < 2000; i++ {
		kind := draw(rng, 4)
		parts := []string{}
		length := 0
		for length < 290 {
			word := words[draw(rng, 12)]
			parts = append(parts, word)
			length += len(word) + 1
		}
		content := "\"" + strings.Join(parts, " ") + "\""
		if draw(rng, 3) == 0 {
			content = "\"" + strings.Join(parts, " ") + "\\n\\\"done\\\"\""
		}
		if kind == 3 {
			content = "null"
		}
		role := "null"
		if kind == 0 {
			role = "\"" + roles[draw(rng, 3)] + "\""
		}
		line := "{\"id\":\"evt_" + strconv.Itoa(i) + "_" + strconv.Itoa(draw(rng, 1000000)) + "\""
		line = line + ",\"session_id\":\"sess_" + strconv.Itoa(draw(rng, 50)) + "\""
		line = line + ",\"event_type\":\"" + types[kind] + "\""
		line = line + ",\"role\":" + role
		line = line + ",\"content\":" + content
		if kind == 1 || kind == 2 {
			line = line + ",\"tool_name\":\"" + tools[draw(rng, 4)] + "\""
		}
		line = line + ",\"extra\":{\"a\":[1,2,3],\"b\":\"z\"}"
		line = line + ",\"created_at\":" + strconv.Itoa(1700000000000+i*1000+draw(rng, 1000)) + "}"
		lines = append(lines, line)
	}
	return lines
}

func orElse(s *string, fallback string) string {
	if s == nil {
		return fallback
	}
	return *s
}

func Run(lines []string) int {
	check := 0
	for _, line := range lines {
		var e Event
		if err := json.Unmarshal([]byte(line), &e); err != nil {
			panic(err)
		}
		h := e.CreatedAt%100000 + len(e.ID) + len(e.SessionID) + len(e.EventType)
		h += len(orElse(e.Role, "x")) + len(orElse(e.Content, "nu")) + len(orElse(e.ToolName, "abc"))
		check = (check + h) % 1000000007
	}
	return check
}

func Main() int {
	return Run(Setup())
}
