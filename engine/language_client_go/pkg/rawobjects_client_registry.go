package baml

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/serde"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type clientProperty struct {
	provider    string
	retryPolicy *string
	options     map[string]any
}

type clientRegistryMap map[string]clientProperty

type ClientRegistry struct {
	primary *string
	clients clientRegistryMap
}

func (c *ClientRegistry) AddLlmClient(name string, provider string, options map[string]any) {
	if c.clients == nil {
		c.clients = make(clientRegistryMap)
	}

	c.clients[name] = clientProperty{
		provider: provider,
		options:  options,
	}
}

func (c *ClientRegistry) SetPrimaryClient(name string) {
	c.primary = &name
}

func encodeClientRegistry(clientRegistryVal *ClientRegistry) (*cffi.HostClientRegistry, error) {
	clientOffsets := make([]*cffi.HostClientProperty, 0, len(clientRegistryVal.clients))
	for name, client := range clientRegistryVal.clients {
		options, err := serde.EncodeMapEntries(client.options, "client options")
		if err != nil {
			return nil, fmt.Errorf("encoding client options: %w", err)
		}
		clientOffsets = append(clientOffsets, &cffi.HostClientProperty{
			Name:        name,
			Provider:    client.provider,
			RetryPolicy: client.retryPolicy,
			Options:     options,
		})
	}

	clients := cffi.HostClientRegistry{
		Clients: clientOffsets,
		Primary: clientRegistryVal.primary,
	}

	return &clients, nil
}
