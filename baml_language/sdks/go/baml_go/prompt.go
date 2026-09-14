package baml_go

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/boundaryml/baml-go/internal/cffi"
	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/proto"
)

// Prompt is the portable bridge representation of an ai.Prompt value. It owns
// a copied prompt tree rather than a runtime handle, so it can be persisted or
// passed into a different BAML runtime without retaining engine identity.
type Prompt struct {
	value *cffi.BamlValuePromptAst
}

// PromptMessage is the structural view returned by Prompt.Messages.
type PromptMessage struct {
	Role     string
	Content  string
	Parts    []any
	Metadata map[string]any
}

func (prompt Prompt) BAMLInput() Input { return PromptInput(prompt) }

func PromptBAMLType() BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_PromptAst{PromptAst: &cffi.BamlTyPromptAst{}}}}
}

func (Prompt) BAMLType() BAMLType { return PromptBAMLType() }

func (Prompt) BAMLDecode(value Value) (any, error) { return value.Prompt() }

func PromptInput(prompt Prompt) Input {
	if prompt.value == nil || prompt.value.Value == nil {
		return InvalidInput("uninitialized BAML Prompt")
	}
	return Input{value: &cffi.InboundValue{Value: &cffi.InboundValue_PromptAstValue{
		PromptAstValue: proto.Clone(prompt.value).(*cffi.BamlValuePromptAst),
	}}}
}

// MarshalJSON exposes a detached, portable form without leaking protobuf
// implementation types through the public API.
func (prompt Prompt) MarshalJSON() ([]byte, error) {
	if prompt.value == nil || prompt.value.Value == nil {
		return nil, fmt.Errorf("cannot serialize an uninitialized BAML Prompt")
	}
	return protojson.Marshal(prompt.value)
}

func (prompt *Prompt) UnmarshalJSON(payload []byte) error {
	if prompt == nil {
		return fmt.Errorf("cannot decode a BAML Prompt into a nil receiver")
	}
	value := &cffi.BamlValuePromptAst{}
	if err := protojson.Unmarshal(payload, value); err != nil {
		return fmt.Errorf("decode BAML Prompt: %w", err)
	}
	if value.Value == nil {
		return fmt.Errorf("decode BAML Prompt: prompt tree is empty")
	}
	prompt.value = value
	return nil
}

var _ json.Marshaler = Prompt{}
var _ json.Unmarshaler = (*Prompt)(nil)

// Prompt decodes a canonical prompt payload. The returned value is detached
// from the result envelope and can be re-entered through PromptInput.
func (value Value) Prompt() (Prompt, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return Prompt{}, err
	}
	if unwrapped.value == nil {
		return Prompt{}, fmt.Errorf("BAML value is uninitialized")
	}
	item, ok := unwrapped.value.Value.(*cffi.BamlOutboundValue_PromptAstValue)
	if !ok || item.PromptAstValue == nil || item.PromptAstValue.Value == nil {
		return Prompt{}, fmt.Errorf("expected BAML Prompt, got %T", unwrapped.value.Value)
	}
	return Prompt{value: proto.Clone(item.PromptAstValue).(*cffi.BamlValuePromptAst)}, nil
}

// Text renders this portable prompt through the canonical ai.Prompt method.
// The prompt remains reusable because every call sends a fresh payload copy.
func (prompt Prompt) Text(ctx context.Context) (string, error) {
	value, err := Call(ctx, "ai.Prompt.text", map[string]Input{"self": prompt.BAMLInput()})
	if err != nil {
		return "", err
	}
	return value.String()
}

// Messages returns the canonical structural message projection.
func (prompt Prompt) Messages(ctx context.Context) ([]PromptMessage, error) {
	value, err := Call(ctx, "ai.Prompt.messages", map[string]Input{"self": prompt.BAMLInput()})
	if err != nil {
		return nil, err
	}
	return DecodeList(value, decodePromptMessage)
}

func decodePromptMessage(value Value) (PromptMessage, error) {
	class, err := value.Class("ai.PromptMessage")
	if err != nil {
		return PromptMessage{}, err
	}
	role, err := class.String("role")
	if err != nil {
		return PromptMessage{}, err
	}
	content, err := class.String("content")
	if err != nil {
		return PromptMessage{}, err
	}
	partsValue, err := class.Field("parts")
	if err != nil {
		return PromptMessage{}, err
	}
	parts, err := DecodeList(partsValue, func(part Value) (any, error) {
		return decodeDynamicValue(part, "ai.PromptMessage.parts", 0)
	})
	if err != nil {
		return PromptMessage{}, err
	}
	metadataValue, err := class.Field("metadata")
	if err != nil {
		return PromptMessage{}, err
	}
	metadata, err := metadataValue.JSON()
	if err != nil {
		return PromptMessage{}, err
	}
	metadataMap, ok := metadata.(map[string]any)
	if !ok {
		return PromptMessage{}, fmt.Errorf("ai.PromptMessage metadata has Go type %T", metadata)
	}
	return PromptMessage{
		Role:     role,
		Content:  content,
		Parts:    parts,
		Metadata: metadataMap,
	}, nil
}
