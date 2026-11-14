package main

import (
	"encoding/json"
	"errors"
	"strconv"
	"strings"
)

// Action is the union Search | Calc | Finish.
type Action interface{ action() }

type Search struct {
	Query string
	Limit *int
}

type Calc struct {
	Op   string
	A, B int
}

type Finish struct {
	Answer     string
	Confidence string
}

func (*Search) action() {}
func (*Calc) action()   {}
func (*Finish) action() {}

// wireAction is every field of every variant; a pointer is nil when the field
// is absent, so a missing required field can be told from a zero value.
type wireAction struct {
	Tool       string  `json:"tool"`
	Query      *string `json:"query"`
	Limit      *int    `json:"limit"`
	Op         *string `json:"op"`
	A          *int    `json:"a"`
	B          *int    `json:"b"`
	Answer     *string `json:"answer"`
	Confidence *string `json:"confidence"`
}

var errNoVariant = errors.New("no Action variant matches")

// decodeAction is the typed decode of one response into the Action union.
func decodeAction(raw string) (Action, error) {
	var w wireAction
	if err := json.Unmarshal([]byte(raw), &w); err != nil {
		return nil, err
	}
	switch w.Tool {
	case "search":
		if w.Query != nil {
			return &Search{Query: *w.Query, Limit: w.Limit}, nil
		}
	case "calc":
		if w.Op != nil && w.A != nil && w.B != nil {
			switch *w.Op {
			case "add", "sub", "mul":
				return &Calc{Op: *w.Op, A: *w.A, B: *w.B}, nil
			}
		}
	case "finish":
		if w.Answer != nil && w.Confidence != nil {
			switch *w.Confidence {
			case "low", "medium", "high":
				return &Finish{Answer: *w.Answer, Confidence: *w.Confidence}, nil
			}
		}
	}
	return nil, errNoVariant
}

type AgentState struct {
	scratch map[string]int
	history []string
	steps   int
	retries int
	answer  *string
}

type AgentFailed struct {
	retries int
}

func (e *AgentFailed) Error() string {
	return "agent failed after " + strconv.Itoa(e.retries) + " retries"
}

func fakeSearch(query string, limit int) int {
	h := 0
	for _, c := range query {
		h = (h*31 + int(c)) % 1000003
	}
	return h % (limit * 100)
}

func step(state *AgentState, action Action) bool {
	state.steps++
	switch action := action.(type) {
	case *Search:
		limit := 10
		if action.Limit != nil {
			limit = *action.Limit
		}
		value := fakeSearch(action.Query, limit)
		state.scratch[action.Query] = value
		state.history = append(state.history, "search:"+action.Query+"="+strconv.Itoa(value))
		return false
	case *Calc:
		value := 0
		switch action.Op {
		case "add":
			value = action.A + action.B
		case "sub":
			value = action.A - action.B
		case "mul":
			value = action.A * action.B
		}
		state.scratch["calc"+strconv.Itoa(state.steps)] = value
		state.history = append(state.history, "calc:"+action.Op+"="+strconv.Itoa(value))
		return false
	case *Finish:
		answer := action.Answer + "/" + action.Confidence
		state.answer = &answer
		state.history = append(state.history, "finish")
		return true
	}
	panic("unreachable")
}

func runConversation(turns []string) (*AgentState, error) {
	state := &AgentState{scratch: map[string]int{}, history: []string{}}
	for _, raw := range turns {
		action, err := decodeAction(raw)
		if err != nil {
			state.retries++
			state.history = append(state.history, "retry")
			if state.retries > 2 {
				return nil, &AgentFailed{retries: state.retries}
			}
		} else {
			if step(state, action) {
				break
			}
		}
	}
	return state, nil
}

// The recorded responses: 300 conversations of 2-7 tool calls ending in a
// finish, with truncated JSON, unknown tools and missing fields mixed in,
// built from a seeded LCG.
type Rng struct {
	seed int
}

func (rng *Rng) draw(n int) int {
	rng.seed = rng.seed * 16807 % 2147483647
	return rng.seed % n
}

func conversations(count int) [][]string {
	rng := &Rng{seed: 17}
	words := []string{"weather", "paris", "capital", "population", "rust", "python", "llm", "agent", "tokens", "latency"}
	ops := []string{"add", "sub", "mul"}
	levels := []string{"low", "medium", "high"}
	convs := [][]string{}
	for c := 0; c < count; c++ {
		turns := []string{}
		n := 2 + rng.draw(6)
		for t := 0; t < n; t++ {
			kind := rng.draw(10)
			if kind < 4 {
				q := []string{}
				m := 1 + rng.draw(3)
				for k := 0; k < m; k++ {
					q = append(q, words[rng.draw(10)])
				}
				r := "{\"tool\": \"search\", \"query\": \"" + strings.Join(q, " ") + "\""
				if rng.draw(2) == 1 {
					r = r + ", \"limit\": " + strconv.Itoa(1+rng.draw(9))
				}
				turns = append(turns, r+"}")
			} else if kind < 7 {
				op := ops[rng.draw(3)]
				a := rng.draw(1000)
				b := rng.draw(1000)
				turns = append(turns, "{\"tool\": \"calc\", \"op\": \""+op+"\", \"a\": "+strconv.Itoa(a)+", \"b\": "+strconv.Itoa(b)+"}")
			} else if kind == 7 {
				turns = append(turns, "{\"tool\": \"search\", \"query\": \"broken\"}"[0:5+rng.draw(10)])
			} else if kind == 8 {
				turns = append(turns, "{\"tool\": \"browse\", \"url\": \"http://x\"}")
			} else {
				turns = append(turns, "{\"tool\": \"calc\", \"op\": \"add\", \"a\": "+strconv.Itoa(rng.draw(10))+"}")
			}
		}
		answer := rng.draw(100000)
		turns = append(turns, "{\"tool\": \"finish\", \"answer\": \"answer "+strconv.Itoa(answer)+"\", \"confidence\": \""+levels[rng.draw(3)]+"\"}")
		convs = append(convs, turns)
	}
	return convs
}

func Main() int {
	convs := conversations(300)
	check := 0
	for pass := 0; pass < 3; pass++ {
		for _, turns := range convs {
			score := 0
			state, err := runConversation(turns)
			if err != nil {
				var failed *AgentFailed
				if !errors.As(err, &failed) {
					panic(err)
				}
				score = 7 + failed.retries
			} else {
				total := 0
				for _, v := range state.scratch {
					total += v
				}
				answer := ""
				if state.answer != nil {
					answer = *state.answer
				}
				score = state.steps*1000 + state.retries*100 + len(answer) + len(state.history)*3 + (total%9973+9973)%9973
			}
			check = (check*31 + score) % 1000000007
		}
	}
	return check
}
