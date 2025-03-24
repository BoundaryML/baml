module example.com/integ-tests

go 1.24.0

require github.com/boundaryml/baml/go v0.2.0

require (
	github.com/davecgh/go-spew v1.1.1 // indirect
	github.com/google/flatbuffers v25.2.10+incompatible // indirect
	github.com/pmezard/go-difflib v1.0.0 // indirect
	github.com/stretchr/testify v1.10.0 // indirect
	gopkg.in/yaml.v3 v3.0.1 // indirect
)

replace github.com/boundaryml/baml/go => ../../engine/language_client_go/go-sdk
