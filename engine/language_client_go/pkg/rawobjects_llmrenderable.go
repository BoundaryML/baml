package baml

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
)

type llmRenderableObject struct {
	// never owns this.
	raw_objects.RawPointer
}

// Description sets the description for the enum
func (eb *llmRenderableObject) SetDescription(description string) error {
	args := map[string]interface{}{
		"description": description,
	}
	_, err := raw_objects.CallMethod(eb, "set_description", args)
	return err
}

func (eb *llmRenderableObject) Description() (*string, error) {
	result, err := raw_objects.CallMethod(eb, "description", nil)
	if err != nil {
		return nil, err
	}

	if result == nil {
		return nil, nil
	}

	description, ok := result.(string)
	if !ok {
		return nil, nil
	}

	return &description, nil
}

// Alias sets the alias for the enum
func (eb *llmRenderableObject) SetAlias(alias string) error {
	args := map[string]interface{}{
		"alias": alias,
	}
	_, err := raw_objects.CallMethod(eb, "set_alias", args)
	return err
}

func (eb *llmRenderableObject) Alias() (*string, error) {

	result, err := raw_objects.CallMethod(eb, "alias", nil)
	if err != nil {
		return nil, err
	}

	if result == nil {
		return nil, nil
	}

	alias, ok := result.(string)
	if !ok {
		return nil, nil
	}

	return &alias, nil
}


func (eb *llmRenderableObject) From() (ASTNodeSource, error) {
	result, err := raw_objects.CallMethod(eb, "is_from_ast", nil)
	if err != nil {
		return ASTNodeSource_Unknown, err
	}

	fromSource, ok := result.(bool)
	if !ok {
		return ASTNodeSource_Unknown, fmt.Errorf("unexpected type from source: %T", result)
	}

	if fromSource {
		return ASTNodeSource_Baml, nil
	}

	return ASTNodeSource_TypeBuilder, nil
}



func (eb *llmRenderableObject) Name() (string, error) {
	result, err := raw_objects.CallMethod(eb, "name", nil)
	if err != nil {
		return "", err
	}

	name, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type from source: %T", result)
	}

	return name, nil
}