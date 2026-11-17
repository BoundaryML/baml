package main

import (
	"encoding/json"
	"strconv"
	"strings"
)

type InputText struct {
	Type string `json:"type"`
	Text string `json:"text"`
}

type InputImage struct {
	Type     string  `json:"type"`
	ImageURL *string `json:"image_url"`
	FileID   *string `json:"file_id"`
	Detail   *string `json:"detail"`
}

type InputPart interface {
	inputPart()
}

func (*InputText) inputPart()  {}
func (*InputImage) inputPart() {}

type MessageItem struct {
	Role    string      `json:"role"`
	Content []InputPart `json:"content"`
}

type FunctionCallItem struct {
	Type      string `json:"type"`
	CallID    string `json:"call_id"`
	Name      string `json:"name"`
	Arguments string `json:"arguments"`
}

type FunctionCallOutputItem struct {
	Type   string `json:"type"`
	CallID string `json:"call_id"`
	Output string `json:"output"`
}

type InputItem interface {
	inputItem()
}

func (*MessageItem) inputItem()            {}
func (*FunctionCallItem) inputItem()       {}
func (*FunctionCallOutputItem) inputItem() {}

type FunctionTool struct {
	Type        string `json:"type"`
	Name        string `json:"name"`
	Description string `json:"description"`
	Parameters  any    `json:"parameters"`
	Strict      bool   `json:"strict"`
}

type Reasoning struct {
	Effort  *string `json:"effort"`
	Summary *string `json:"summary"`
}

type Request struct {
	Model             string            `json:"model"`
	Instructions      *string           `json:"instructions"`
	Input             []InputItem       `json:"input"`
	Tools             []*FunctionTool   `json:"tools"`
	ToolChoice        *string           `json:"tool_choice"`
	Temperature       *float64          `json:"temperature"`
	TopP              *float64          `json:"top_p"`
	MaxOutputTokens   int               `json:"max_output_tokens"`
	ParallelToolCalls bool              `json:"parallel_tool_calls"`
	Reasoning         *Reasoning        `json:"reasoning"`
	Metadata          map[string]string `json:"metadata"`
	Store             bool              `json:"store"`
	User              *string           `json:"user"`
	Stream            bool              `json:"stream"`
}

// encoding/json decides from the field tag whether a nil field is written as
// null or left out, so the `omit_nulls` form of the request is a second set of
// types for the classes that have optional fields, tagged `omitempty`.
type inputImageOmitNulls struct {
	Type     string  `json:"type"`
	ImageURL *string `json:"image_url,omitempty"`
	FileID   *string `json:"file_id,omitempty"`
	Detail   *string `json:"detail,omitempty"`
}

func (*inputImageOmitNulls) inputPart() {}

type reasoningOmitNulls struct {
	Effort  *string `json:"effort,omitempty"`
	Summary *string `json:"summary,omitempty"`
}

type requestOmitNulls struct {
	Model             string              `json:"model"`
	Instructions      *string             `json:"instructions,omitempty"`
	Input             []InputItem         `json:"input"`
	Tools             []*FunctionTool     `json:"tools"`
	ToolChoice        *string             `json:"tool_choice,omitempty"`
	Temperature       *float64            `json:"temperature,omitempty"`
	TopP              *float64            `json:"top_p,omitempty"`
	MaxOutputTokens   int                 `json:"max_output_tokens"`
	ParallelToolCalls bool                `json:"parallel_tool_calls"`
	Reasoning         *reasoningOmitNulls `json:"reasoning,omitempty"`
	Metadata          map[string]string   `json:"metadata"`
	Store             bool                `json:"store"`
	User              *string             `json:"user,omitempty"`
	Stream            bool                `json:"stream"`
}

// omitNulls views the request through the `omitempty` types. The image parts
// and the reasoning are converted in place (the types differ only in tags);
// the message items that hold them get a new content list.
func omitNulls(request *Request) *requestOmitNulls {
	input := make([]InputItem, len(request.Input))
	for i, item := range request.Input {
		if message, ok := item.(*MessageItem); ok {
			content := make([]InputPart, len(message.Content))
			for p, part := range message.Content {
				if image, ok := part.(*InputImage); ok {
					part = (*inputImageOmitNulls)(image)
				}
				content[p] = part
			}
			item = &MessageItem{Role: message.Role, Content: content}
		}
		input[i] = item
	}
	return &requestOmitNulls{
		Model:             request.Model,
		Instructions:      request.Instructions,
		Input:             input,
		Tools:             request.Tools,
		ToolChoice:        request.ToolChoice,
		Temperature:       request.Temperature,
		TopP:              request.TopP,
		MaxOutputTokens:   request.MaxOutputTokens,
		ParallelToolCalls: request.ParallelToolCalls,
		Reasoning:         (*reasoningOmitNulls)(request.Reasoning),
		Metadata:          request.Metadata,
		Store:             request.Store,
		User:              request.User,
		Stream:            request.Stream,
	}
}

type Rng struct {
	seed int
}

func draw(rng *Rng, n int) int {
	rng.seed = rng.seed * 16807 % 2147483647
	return rng.seed % n
}

func sentence(rng *Rng, words []string, count int) string {
	parts := []string{}
	for i := 0; i < count; i++ {
		parts = append(parts, words[draw(rng, len(words))])
	}
	return strings.Join(parts, " ")
}

func toolSchema(rng *Rng, words []string, fields int) map[string]any {
	properties := map[string]any{}
	required := []any{}
	for f := 0; f < fields; f++ {
		name := words[draw(rng, len(words))] + "_" + strconv.Itoa(f)
		kind := draw(rng, 4)
		if kind == 0 {
			properties[name] = map[string]any{"type": "string", "description": sentence(rng, words, 6)}
		} else if kind == 1 {
			properties[name] = map[string]any{"type": "integer", "minimum": 0, "maximum": 1000 + draw(rng, 9000)}
		} else if kind == 2 {
			properties[name] = map[string]any{"type": "array", "items": map[string]any{"type": "string", "enum": []any{"low", "medium", "high"}}}
		} else {
			properties[name] = map[string]any{
				"type":                 "object",
				"properties":           map[string]any{"lat": map[string]any{"type": "number"}, "lng": map[string]any{"type": "number"}},
				"required":             []any{"lat", "lng"},
				"additionalProperties": false,
			}
		}
		if draw(rng, 3) != 0 {
			required = append(required, name)
		}
	}
	return map[string]any{"type": "object", "properties": properties, "required": required, "additionalProperties": false}
}

func ptr[T any](value T) *T {
	return &value
}

func Setup() *Request {
	rng := &Rng{seed: 29}
	words := []string{"weather", "forecast", "city", "units", "latency", "budget", "invoice", "customer", "order", "status", "refund", "search", "query", "limit", "region", "timezone"}
	input := []InputItem{}
	for i := 0; i < 24; i++ {
		kind := draw(rng, 4)
		if kind < 2 {
			parts := []InputPart{}
			count := 1 + draw(rng, 3)
			for p := 0; p < count; p++ {
				if draw(rng, 4) == 0 {
					image := &InputImage{
						Type:     "input_image",
						ImageURL: ptr("https://example.com/images/" + strconv.Itoa(draw(rng, 100000)) + ".png"),
					}
					if draw(rng, 2) == 0 {
						image.Detail = ptr("auto")
					}
					parts = append(parts, image)
				} else {
					parts = append(parts, &InputText{Type: "input_text", Text: sentence(rng, words, 12+draw(rng, 30))})
				}
			}
			role := "assistant"
			if kind == 0 {
				role = "user"
			}
			input = append(input, &MessageItem{Role: role, Content: parts})
		} else if kind == 2 {
			call := "call_" + strconv.Itoa(draw(rng, 1000000))
			name := words[draw(rng, len(words))]
			query := sentence(rng, words, 3)
			input = append(input, &FunctionCallItem{
				Type:      "function_call",
				CallID:    call,
				Name:      name,
				Arguments: "{\"query\":\"" + query + "\",\"limit\":" + strconv.Itoa(draw(rng, 50)) + "}",
			})
		} else {
			call := "call_" + strconv.Itoa(draw(rng, 1000000))
			input = append(input, &FunctionCallOutputItem{
				Type:   "function_call_output",
				CallID: call,
				Output: sentence(rng, words, 20),
			})
		}
	}
	tools := []*FunctionTool{}
	for t := 0; t < 8; t++ {
		name := "tool_" + words[draw(rng, len(words))] + "_" + strconv.Itoa(t)
		description := sentence(rng, words, 10)
		parameters := toolSchema(rng, words, 2+draw(rng, 5))
		tools = append(tools, &FunctionTool{
			Type:        "function",
			Name:        name,
			Description: description,
			Parameters:  parameters,
			Strict:      draw(rng, 2) == 0,
		})
	}
	session := "sess_" + strconv.Itoa(draw(rng, 1000))
	trace := "tr_" + strconv.Itoa(draw(rng, 100000))
	return &Request{
		Model:             "gpt-5.1-mini",
		Instructions:      nil,
		Input:             input,
		Tools:             tools,
		ToolChoice:        ptr("auto"),
		Temperature:       ptr(0.7),
		TopP:              nil,
		MaxOutputTokens:   4096,
		ParallelToolCalls: true,
		Reasoning:         &Reasoning{Effort: ptr("low"), Summary: nil},
		Metadata:          map[string]string{"session": session, "trace": trace},
		Store:             false,
		User:              nil,
		Stream:            true,
	}
}

func Run(request *Request) int {
	full, err := json.Marshal(request)
	if err != nil {
		panic(err)
	}
	compact, err := json.Marshal(omitNulls(request))
	if err != nil {
		panic(err)
	}
	return len(full)*3 + len(compact)
}

func Main() int {
	return Run(Setup())
}
