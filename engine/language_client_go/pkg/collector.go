package baml

import (
	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
)

type Collector = raw_objects.Collector

func NewCollector(name string) (Collector, error) {
	return raw_objects.NewCollector(name)
}
