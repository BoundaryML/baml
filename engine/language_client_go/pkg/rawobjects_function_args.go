package baml

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/baml_go/serde"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
	"google.golang.org/protobuf/proto"
)

type BamlFunctionArguments struct {
	Kwargs         map[string]any
	ClientRegistry *ClientRegistry
	Env            map[string]string
	Collectors     []Collector
	TypeBuilder    TypeBuilder
}

func (args *BamlFunctionArguments) Encode() ([]byte, error) {
	encoded, err := args.encode()
	if err != nil {
		return nil, err
	}
	return proto.Marshal(encoded)
}

func (args *BamlFunctionArguments) encode() (*cffi.CFFIFunctionArguments, error) {
	kwargs, err := serde.EncodeMapEntries(args.Kwargs, "function arguments")
	if err != nil {
		return nil, fmt.Errorf("encoding function arguments: %w", err)
	}

	var clientRegistry *cffi.CFFIClientRegistry
	if args.ClientRegistry != nil {
		clientRegistry, err = encodeClientRegistry(args.ClientRegistry)
		if err != nil {
			return nil, fmt.Errorf("encoding client registry: %w", err)
		}
	}

	var env []*cffi.CFFIEnvVar
	if args.Env != nil {
		env, err = serde.EncodeEnvVar(args.Env)
		if err != nil {
			return nil, fmt.Errorf("encoding env vars: %w", err)
		}
	}

	var collectors []*cffi.CFFIRawObject
	if args.Collectors != nil {
		for _, collector := range args.Collectors {
			if collector == nil {
				return nil, fmt.Errorf("nil collector found in collectors")
			}
			encodedCollector := raw_objects.EncodeRawObject(collector)
			if err != nil {
				return nil, fmt.Errorf("encoding collector: %w", err)
			}
			collectors = append(collectors, encodedCollector)
		}
	}

	var typeBuilder *cffi.CFFIRawObject
	if args.TypeBuilder != nil {
		encodedTypeBuilder := raw_objects.EncodeRawObject(args.TypeBuilder)
		if err != nil {
			return nil, fmt.Errorf("encoding type builder: %w", err)
		}
		typeBuilder = encodedTypeBuilder
	}

	functionArguments := cffi.CFFIFunctionArguments{
		Kwargs:         kwargs,
		ClientRegistry: clientRegistry,
		Env:            env,
		Collectors:     collectors,
		TypeBuilder:    typeBuilder,
	}

	return &functionArguments, nil
}
