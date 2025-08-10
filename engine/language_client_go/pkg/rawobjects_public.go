package baml

import (
	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/baml_go/serde"
)

type ASTNodeSource string

const (
	ASTNodeSource_Unknown ASTNodeSource = "unknown"
	ASTNodeSource_Baml    ASTNodeSource = "baml_file"
	ASTNodeSource_TypeBuilder ASTNodeSource = "type_builder"
)

type MediaType string

const (
	MediaType_Image MediaType = "Image"
	MediaType_Audio MediaType = "Audio"
	MediaType_PDF   MediaType = "PDF"
	MediaType_Video MediaType = "Video"
)

type media interface {
	raw_objects.RawPointer
	serde.InternalBamlSerializer
	MediaType() (MediaType, error)
	MimeType() (*string, error)
	AsUrl() (*string, error)
	AsBase64() (*string, error)
	IsUrl() (bool, error)
	IsBase64() (bool, error)
}

type Image interface {
	media
}

type Audio interface {
	media
}

type PDF interface {
	media
}

type Video interface {
	media
}

type Collector interface {
	raw_objects.RawPointer
	// Usage gathers the Usage object but keeps the underlying C collector alive
	// until the Usage is done.
	Usage() (Usage, error)
	// Name returns the collector name
	Name() (string, error)
	// Logs returns all function logs
	Logs() ([]FunctionLog, error)
	// Last returns the most recent function log
	Last() (FunctionLog, error)
	// ID looks up a function log by ID
	Id(functionId string) (FunctionLog, error)
	// Clear removes all logs and frees memory
	Clear() (int64, error)
}

type Usage interface {
	raw_objects.RawPointer
	// InputTokens returns the number of input tokens
	InputTokens() (int64, error)
	// OutputTokens returns the number of output tokens
	OutputTokens() (int64, error)
}

type FunctionLog interface {
	raw_objects.RawPointer
	// ID returns the function log ID
	Id() (string, error)
	// FunctionName returns the function name
	FunctionName() (string, error)
	// LogType returns the log type
	LogType() (string, error)
	// Timing returns the timing information
	Timing() (Timing, error)
	// Usage returns the usage information
	Usage() (Usage, error)
	// RawLLMResponse returns the raw LLM response
	RawLLMResponse() (string, error)
	// Calls returns all LLM calls made during this function execution (can be LLMCall or LLMStreamCall)
	Calls() ([]LLMCall, error)
	// SelectedCall returns the call that was selected for parsing (can be LLMCall or LLMStreamCall)
	SelectedCall() (LLMCall, error)
	// Tags returns any user-provided metadata
	Tags() (map[string]any, error)
}

type Timing interface {
	raw_objects.RawPointer
	// StartTimeUTCMs returns the start time in milliseconds since epoch
	StartTimeUTCMs() (int64, error)
	// DurationMs returns the duration in milliseconds (nullable)
	DurationMs() (*int64, error)
}

type StreamTiming interface {
	Timing
}

type LLMCall interface {
	raw_objects.RawPointer
	// ID returns the request ID: Not the same as the function log ID
	RequestId() (string, error)
	// ClientName returns the name of the client used
	ClientName() (string, error)
	// Provider returns the provider of the client
	Provider() (string, error)
	// HttpRequest returns the raw HTTP request
	HttpRequest() (HTTPRequest, error)
	// HttpResponse returns the raw HTTP response (nullable for streaming)
	HttpResponse() (HTTPResponse, error)
	// Usage returns the usage information (nullable)
	Usage() (Usage, error)
	// Selected returns whether this call was selected for parsing
	Selected() (bool, error)
	// Timing returns the timing information
	Timing() (Timing, error)
}

type LLMStreamCall interface {
	LLMCall
	// SSEChunks returns the SSE chunks of the response
	SSEChunks() ([]SSEResponse, error)
}

type HTTPRequest interface {
	raw_objects.RawPointer
	// ID returns the request ID
	RequestId() (string, error)
	// URL returns the request URL
	Url() (string, error)
	// Method returns the HTTP method
	Method() (string, error)
	// Headers returns the request headers
	Headers() (map[string]string, error)
	// Body returns the request body
	Body() (HTTPBody, error)
}

type HTTPResponse interface {
	raw_objects.RawPointer
	// ID returns the request ID
	RequestId() (string, error)
	// Status returns the HTTP status code
	Status() (int64, error)
	// Headers returns the response headers
	Headers() (map[string]string, error)
	// Body returns the response body
	Body() (HTTPBody, error)
}

type HTTPBody interface {
	raw_objects.RawPointer
	// Text returns the body as a string
	Text() (string, error)
	// JSON returns the body as a JSON object
	JSON() (any, error)
}

type SSEResponse interface {
	raw_objects.RawPointer
	// Text returns the body as a string
	Text() (string, error)
	// JSON returns the body as a JSON object (nullable)
	JSON() (any, error)
}

type TickReason string

const (
	TickReason_Unknown TickReason = "Unknown"
)

// Types for BAML Type Construction

type TypeBuilder interface {
	raw_objects.RawPointer
	// Basic types
	String() (Type, error)
	Int() (Type, error)
	Float() (Type, error)
	Bool() (Type, error)
	Null() (Type, error)
	// Literal types  
	LiteralString(value string) (Type, error)
	LiteralInt(value int64) (Type, error)
	LiteralBool(value bool) (Type, error)
	// Composite types
	Map(key Type, value Type) (Type, error)
	List(inner Type) (Type, error)
	Optional(inner Type) (Type, error)
	Union(types []Type) (Type, error)
	// BAML schema operations
	AddBaml(baml string) error
	// Enum operations
	AddEnum(name string) (EnumBuilder, error)
	Enum(name string) (EnumBuilder, error)
	ListEnums() ([]EnumBuilder, error)
	// Class operations
	AddClass(name string) (ClassBuilder, error)
	Class(name string) (ClassBuilder, error)
	ListClasses() ([]ClassBuilder, error)

	// Display the type builder
	Print() string
}

type Type interface {
	raw_objects.RawPointer
	serde.InternalBamlSerializer
	// Type extensions
	List() (Type, error)
	Optional() (Type, error)
	Print() string
}

type llmRenderable interface {
	// Set the description for the object
	SetDescription(description string) error
	// Set the alias for the object
	SetAlias(alias string) error
	// Get the description for the object
	Description() (*string, error)
	// Get the alias for the object
	Alias() (*string, error)
	// Determine where this enum was defined
	From() (ASTNodeSource, error)
	// Get the name for the property
	Name() (string, error)
}

type EnumBuilder interface {
	raw_objects.RawPointer
	llmRenderable
	// Add a new value to the enum
	AddValue(value string) (EnumValueBuilder, error)
	// Get the type definition for this enum
	Type() (Type, error)
	// List all values in the enum
	ListValues() ([]EnumValueBuilder, error)
	// Get a specific value from the enum
	Value(name string) (EnumValueBuilder, error)
}

type EnumValueBuilder interface {
	raw_objects.RawPointer
	llmRenderable
	// Mark the enum value to be skipped
	SetSkip(skip bool) error
	// Get the skip value
	Skip() (bool, error)
}

type ClassBuilder interface {
	raw_objects.RawPointer
	llmRenderable
	// Get the type definition for this class
	Type() (Type, error)
	// List all properties in the class
	ListProperties() ([]ClassPropertyBuilder, error)
	// Add a new property to the class
	AddProperty(name string, fieldType Type) (ClassPropertyBuilder, error)
	// Get a specific property from the class
	Property(name string) (ClassPropertyBuilder, error)
}

type ClassPropertyBuilder interface {
	raw_objects.RawPointer
	llmRenderable
	// Set the type for the property
	SetType(fieldType Type) error
	// Get the type for the property
	Type() (Type, error)
}
