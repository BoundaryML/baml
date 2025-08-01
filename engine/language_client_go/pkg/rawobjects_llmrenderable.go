package baml

import "github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"

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
