# BAML User Stories & Feature Checklist

This document contains all features for BAML organized as user stories for recreating the compiler and tooling from scratch.

---

## 1. Language Core (Parser & AST)

### 1.1 Comments
- [ ] As a developer, I can write line comments with `//` syntax
- [ ] As a developer, I can write block comments with `/* */` syntax
- [ ] As a developer, I can nest block comments
- [ ] As a developer, I can use Jinja comments `{# comment #}` in prompts

### 1.2 Primitive Types
- [ ] As a developer, I can use `bool` type for boolean values
- [ ] As a developer, I can use `int` type for integer values
- [ ] As a developer, I can use `float` type for floating-point values
- [ ] As a developer, I can use `string` type for text values
- [ ] As a developer, I can use `null` type for null values

### 1.3 Literal Types
- [ ] As a developer, I can constrain strings to specific literal values (e.g., `"red" | "green" | "blue"`)
- [ ] As a developer, I can constrain integers to specific literal values
- [ ] As a developer, I can constrain booleans to specific literal values (`true` or `false`)
- [ ] As a developer, I can combine literal types with union syntax
- [ ] As a developer, I can mix literal types in unions (e.g., `1 | true | "string output"`)

### 1.4 Container Types
- [ ] As a developer, I can use array types with `Type[]` syntax
- [ ] As a developer, I can use map types with `map<string, Type>` syntax
- [ ] As a developer, I can nest container types (e.g., `string[][]`, `map<string, int[]>`)
- [ ] As a developer, I can use optional container types (`string[]?`, `map<K, V>?`)

### 1.5 Union Types
- [ ] As a developer, I can define union types with `Type1 | Type2` syntax
- [ ] As a developer, I can use unions with primitives, classes, and enums
- [ ] As a developer, I can have functions return union types
- [ ] As a developer, I can use parenthesized unions for complex types (e.g., `(bool[] | int[])`)
- [ ] As a developer, I can use constraints to help disambiguate union variants

### 1.6 Optional Types
- [ ] As a developer, I can mark fields as optional with `?` syntax
- [ ] As a developer, I can use optional types in function parameters
- [ ] As a developer, I can use optional types in return values
- [ ] As a developer, I can use nested optional types

### 1.7 Multimodal Types
- [ ] As a developer, I can use `image` type for image inputs
- [ ] As a developer, I can use `audio` type for audio inputs
- [ ] As a developer, I can use `pdf` type for PDF document inputs
- [ ] As a developer, I can use `video` type for video inputs
- [ ] As a developer, I can provide multimodal inputs via URL
- [ ] As a developer, I can provide multimodal inputs via base64 encoding
- [ ] As a developer, I can provide multimodal inputs via file path
- [ ] As a developer, I can specify `media_type` explicitly for media inputs
- [ ] As a developer, I get automatic media type detection from URLs

### 1.8 String Literals
- [ ] As a developer, I can use inline strings with `"quoted string"` syntax
- [ ] As a developer, I can use single-hash block strings `#" multi-line "#` for prompts
- [ ] As a developer, I can use double-hash block strings `##" ... "##` for different escaping
- [ ] As a developer, I can use raw string literals with proper escape handling

---

## 2. Type Definitions

### 2.1 Classes (Custom Types)
- [ ] As a developer, I can define a class with `class ClassName { }` syntax
- [ ] As a developer, I can define typed fields within a class
- [ ] As a developer, I can nest classes (class fields referencing other classes)
- [ ] As a developer, I can use arrays of classes
- [ ] As a developer, I can use optional class fields with `?`
- [ ] As a developer, I can reference classes in function inputs and outputs
- [ ] As a developer, I can define literal class properties for union discrimination (e.g., `prop "one"`)

### 2.2 Enums
- [ ] As a developer, I can define an enum with `enum EnumName { }` syntax
- [ ] As a developer, I can define enum values (variants)
- [ ] As a developer, I can reference enums in function inputs and outputs
- [ ] As a developer, I can use enums for classification tasks
- [ ] As a developer, I can use enums in union types

### 2.3 Type Aliases
- [ ] As a developer, I can create type aliases with `type AliasName = Type` syntax
- [ ] As a developer, I can create type aliases for complex types
- [ ] As a developer, I can add attributes to type aliases (e.g., `type Currency = int @check(...)`)
- [ ] As a developer, I can use type aliases in class fields and function signatures

### 2.4 Recursive Types
- [ ] As a developer, I can define recursive classes (class referencing itself)
- [ ] As a developer, I can define mutually recursive classes (A references B, B references A)
- [ ] As a developer, I can define recursive type aliases (e.g., `type JsonValue = map<string, JsonValue>`)
- [ ] As a developer, I can use recursive types through intermediate types

---

## 3. Attributes System

### 3.1 Field-Level Attributes
- [ ] As a developer, I can use `@alias("name")` to rename a field for LLM understanding
- [ ] As a developer, I can use `@description("text")` to add context to a field
- [ ] As a developer, I can use `@skip` to exclude a field from prompts/output schema
- [ ] As a developer, I can use `@assert(name, condition)` for strict validation (raises exception)
- [ ] As a developer, I can use `@check(name, condition)` for non-strict validation (returns results)

### 3.2 Block-Level Attributes
- [ ] As a developer, I can use `@@dynamic` to enable runtime type modification
- [ ] As a developer, I can use `@@alias("name")` for enum-level aliasing
- [ ] As a developer, I can use `@@description("text")` for type-level descriptions
- [ ] As a developer, I can use `@@assert(name, condition)` for class-level assertions
- [ ] As a developer, I can use `@@check(name, condition)` for cross-field validation

### 3.3 Streaming Attributes
- [ ] As a developer, I can use `@stream.done` to mark when a field is complete during streaming
- [ ] As a developer, I can use `@stream.not_null` to mark fields that shouldn't be null in stream
- [ ] As a developer, I can use `@stream.with_state` for streaming state management

### 3.4 Attribute Expressions
- [ ] As a developer, I can use Jinja expressions in `@assert` conditions
- [ ] As a developer, I can use Jinja expressions in `@check` conditions
- [ ] As a developer, I can access `this` variable for the current field value
- [ ] As a developer, I can use Jinja filters in attribute expressions
- [ ] As a developer, I can use `|regex_match()` filter for regex validation
- [ ] As a developer, I can use `|length` filter for length validation
- [ ] As a developer, I can reference other fields in block-level constraints (e.g., `this.field1 < this.field2`)

### 3.5 Attribute Inheritance
- [ ] As a developer, attributes from type aliases merge with field attributes
- [ ] As a developer, constraints are properly propagated through type references

---

## 4. Functions

### 4.1 Function Definition
- [ ] As a developer, I can define a function with `function FunctionName(args) -> ReturnType { }` syntax
- [ ] As a developer, I can define typed input parameters
- [ ] As a developer, I can define the return type
- [ ] As a developer, I can use complex types (classes, enums) in inputs/outputs
- [ ] As a developer, I can reference a client in the function body
- [ ] As a developer, I can add constraints to function parameters
- [ ] As a developer, I can add constraints to function return types

### 4.2 Function Body (Prompt Block)
- [ ] As a developer, I can write prompts in the `prompt` block
- [ ] As a developer, I can use block strings `#" ... "#` for multi-line prompts
- [ ] As a developer, I can use Jinja templating in prompts
- [ ] As a developer, I can access input parameters via `{{ param_name }}`

### 4.3 Client Assignment
- [ ] As a developer, I can assign a client to a function with `client ClientName`
- [ ] As a developer, I can assign client strategies (fallback, round-robin, retry)
- [ ] As a developer, I can use shorthand client syntax `client "provider/model"`

### 4.4 Pure Expression Functions (No LLM)
- [ ] As a developer, I can define functions without a client/prompt (pure computation)
- [ ] As a developer, I can use `let` variable declarations in function bodies
- [ ] As a developer, I can reassign variables with `=` operator
- [ ] As a developer, I can use if/else/else if expressions
- [ ] As a developer, I can use while loops
- [ ] As a developer, I can use for loops with `for (let item in list)` syntax
- [ ] As a developer, I can call other functions from within a function
- [ ] As a developer, I can use arithmetic operators

### 4.5 Built-in Functions
- [ ] As a developer, I can use `baml.fetch_as<T>()` for HTTP requests with typed responses
- [ ] As a developer, I can use `baml.HttpRequest` for constructing HTTP requests
- [ ] As a developer, I can use `baml.HttpMethod` for HTTP methods (GET, POST, PUT, etc.)
- [ ] As a developer, I can use `image.from_url(url)` to construct image inputs

---

## 5. Template Strings

### 5.1 Definition
- [ ] As a developer, I can define template strings with `template_string TemplateName(args) #" ... "#`
- [ ] As a developer, I can define typed parameters for templates
- [ ] As a developer, I can use Jinja syntax within templates

### 5.2 Usage
- [ ] As a developer, I can reference templates in function prompts
- [ ] As a developer, I can nest templates within other templates
- [ ] As a developer, I can pass arguments to templates

---

## 6. Jinja Templating

### 6.1 Variable Interpolation
- [ ] As a developer, I can interpolate variables with `{{ variable }}`
- [ ] As a developer, I can access nested object properties with dot notation
- [ ] As a developer, I can access array elements

### 6.2 Control Flow
- [ ] As a developer, I can use `{% if condition %} ... {% endif %}` conditionals
- [ ] As a developer, I can use `{% elif %}` and `{% else %}` branches
- [ ] As a developer, I can use `{% for item in list %} ... {% endfor %}` loops
- [ ] As a developer, I can access loop variables (`loop.index`, `loop.first`, `loop.last`)

### 6.3 Filters
- [ ] As a developer, I can use standard Jinja filters (e.g., `| upper`, `| lower`, `| length`)
- [ ] As a developer, I can chain multiple filters
- [ ] As a developer, I can use custom BAML-specific filters
- [ ] As a developer, I can use `|regex_match(pattern)` for regex matching

### 6.4 Special BAML Context Variables
- [ ] As a developer, I can use `{{ ctx.output_format }}` to auto-generate format instructions
- [ ] As a developer, I can use `{{ ctx.output_format(map_style='angle') }}` for alternative formatting
- [ ] As a developer, I can use `{{ ctx.client }}` to access the selected client info
- [ ] As a developer, I can use `{{ ctx.client.provider }}` to access provider name
- [ ] As a developer, I can use `{{ ctx.client.model }}` to access model name

### 6.5 Message Roles
- [ ] As a developer, I can use `{{ _.role("system") }}` to define system messages
- [ ] As a developer, I can use `{{ _.role("user") }}` to define user messages
- [ ] As a developer, I can use `{{ _.role("assistant") }}` to define assistant messages
- [ ] As a developer, I can use `{{ _.role("role", metadata={...}) }}` to add role metadata
- [ ] As a developer, I can use metadata for prompt caching (e.g., `cache_control={"type": "ephemeral"}`)

---

## 7. LLM Clients

### 7.1 Client Definition
- [ ] As a developer, I can define a client with `client<llm> ClientName { }` syntax
- [ ] As a developer, I can specify a provider (e.g., `anthropic`, `openai`)
- [ ] As a developer, I can configure provider-specific options
- [ ] As a developer, I can use environment variables for secrets with `env.VAR_NAME`
- [ ] As a developer, I can use shorthand syntax `client "provider/model"` inline

### 7.2 Supported Providers
- [ ] As a developer, I can use OpenAI provider
- [ ] As a developer, I can use OpenAI Responses API provider
- [ ] As a developer, I can use Anthropic provider
- [ ] As a developer, I can use Google AI (Gemini) provider
- [ ] As a developer, I can use Google Vertex provider
- [ ] As a developer, I can use AWS Bedrock provider
- [ ] As a developer, I can use Azure OpenAI provider
- [ ] As a developer, I can use OpenAI-compatible providers (OpenRouter, Groq, Ollama, Together.ai, etc.)
- [ ] As a developer, I can use `openai-generic` for custom OpenAI-compatible endpoints

### 7.3 Client Options - Common
- [ ] As a developer, I can set `model` option
- [ ] As a developer, I can set `api_key` option
- [ ] As a developer, I can set `temperature` option
- [ ] As a developer, I can set `max_tokens` option
- [ ] As a developer, I can set `max_completion_tokens` option
- [ ] As a developer, I can set `max_tokens` to `null` explicitly (for models that don't allow it)
- [ ] As a developer, I can set `base_url` for custom endpoints
- [ ] As a developer, I can set custom `headers` per client
- [ ] As a developer, I can configure `allowed_role_metadata` for secure metadata passing

### 7.4 Client Options - Timeouts
- [ ] As a developer, I can set `request_timeout_ms` for overall request timeout
- [ ] As a developer, I can set `connect_timeout_ms` for connection timeout
- [ ] As a developer, I can set `time_to_first_token_timeout_ms` for streaming first token
- [ ] As a developer, I can set `idle_timeout_ms` for idle connection timeout
- [ ] As a developer, I can set timeout to `0` for infinite (no timeout)

### 7.5 Client Options - Provider Specific
- [ ] Anthropic: I can set `anthropic_version`
- [ ] Anthropic: I can configure `thinking` with budget tokens for extended reasoning
- [ ] Google AI: I can set `safetySettings` for content filtering
- [ ] Google AI: I can set `generationConfig` with `thinkingConfig`
- [ ] Azure: I can set `resource_name`, `deployment_id`, `api_version`
- [ ] AWS Bedrock: I can set `inference_configuration`
- [ ] AWS Bedrock: I can set credentials (`access_key`, `secret_key`, `session_token`, `profile`, `region`)
- [ ] Vertex AI: I can set `credentials` (JSON), `project_id`, `location`
- [ ] OpenRouter: I can set custom headers for app attribution

### 7.6 Client Options - Advanced
- [ ] As a developer, I can set `finish_reason_allow_list` to filter acceptable completion reasons
- [ ] As a developer, I can configure media URL handlers per media type
- [ ] As a developer, I can use `send_base64`, `send_url`, `send_url_add_mime_type` for media handling

---

## 8. Client Strategies

### 8.1 Retry Policy
- [ ] As a developer, I can define a retry policy with `retry_policy PolicyName { }`
- [ ] As a developer, I can set `max_retries` count
- [ ] As a developer, I can use `constant_delay` strategy with `delay_ms`
- [ ] As a developer, I can use `exponential_backoff` strategy
- [ ] As a developer, I can configure `initial_delay_ms`, `multiplier`, `max_delay_ms` for exponential backoff

### 8.2 Fallback Strategy
- [ ] As a developer, I can define a fallback client with `provider fallback` and `strategy [Client1, Client2]`
- [ ] As a developer, I can nest fallback strategies
- [ ] As a developer, I can combine fallback with retry
- [ ] As a developer, I get aggregated error reporting showing all fallback attempts

### 8.3 Round-Robin Strategy
- [ ] As a developer, I can define a round-robin client with `provider baml-round-robin`
- [ ] As a developer, I can specify multiple clients in the rotation
- [ ] As a developer, I can set `start` index for round-robin

---

## 9. Test Blocks

### 9.1 Test Definition
- [ ] As a developer, I can define tests with `test TestName { }` syntax
- [ ] As a developer, I can specify the function to test with `functions [FunctionName]`
- [ ] As a developer, I can test multiple functions in a single test
- [ ] As a developer, I can provide test inputs with `args { }`

### 9.2 Test Inputs
- [ ] As a developer, I can provide primitive test inputs
- [ ] As a developer, I can provide complex object test inputs (nested objects)
- [ ] As a developer, I can provide array test inputs
- [ ] As a developer, I can provide map test inputs
- [ ] As a developer, I can provide multimodal test inputs (images, videos, PDFs, audio)
- [ ] As a developer, I can use `file "../path/to/file"` for file-based test inputs
- [ ] As a developer, I can specify `media_type` in test inputs

### 9.3 Test Validation
- [ ] As a developer, I can use `@@assert` in tests for validation
- [ ] As a developer, I can use `@@check` in tests for non-strict validation
- [ ] As a developer, I can validate test outputs match expected types

### 9.4 Test Type Builder
- [ ] As a developer, I can use TypeBuilder to customize schemas at test time
- [ ] As a developer, I can add dynamic enum values in tests
- [ ] As a developer, I can add dynamic class fields in tests

---

## 10. Generator Configuration

### 10.1 Generator Block
- [ ] As a developer, I can define a generator with `generator LanguageName { }` syntax
- [ ] As a developer, I can specify `output_type` (python, typescript, go, ruby, rest/openapi)
- [ ] As a developer, I can specify `output_dir` for generated code location
- [ ] As a developer, I can specify `version` for code generation compatibility

### 10.2 Language-Specific Options
- [ ] As a developer, I can configure Python-specific generation options
- [ ] As a developer, I can configure TypeScript-specific generation options
- [ ] As a developer, I can configure Go-specific generation options
- [ ] As a developer, I can configure Ruby-specific generation options

---

## 11. Code Generation

### 11.1 Type Generation
- [ ] As a developer, I get generated classes/structs for all BAML classes
- [ ] As a developer, I get generated enums for all BAML enums
- [ ] As a developer, I get proper type mappings for primitives
- [ ] As a developer, I get proper type mappings for containers (arrays, maps)
- [ ] As a developer, I get proper type mappings for optional types
- [ ] As a developer, I get proper type mappings for union types
- [ ] As a developer, I get proper type mappings for recursive types

### 11.2 Function Wrappers
- [ ] As a developer, I get async function wrappers for all BAML functions
- [ ] As a developer, I get sync function wrappers for all BAML functions (where applicable)
- [ ] As a developer, I get proper parameter typing in generated functions
- [ ] As a developer, I get proper return type typing in generated functions

### 11.3 Streaming Support
- [ ] As a developer, I get streaming variants of functions (`b.stream.FunctionName`)
- [ ] As a developer, I get partial types for incremental response parsing
- [ ] As a developer, I can iterate over stream chunks
- [ ] As a developer, I can get the final response from a stream

### 11.4 Request/Parse API
- [ ] As a developer, I can generate raw HTTP requests (`b.request.FunctionName`)
- [ ] As a developer, I can parse LLM responses manually (`b.parse.FunctionName`)

### 11.5 Language-Specific Features
- [ ] Python: Generated code uses Pydantic models
- [ ] Python: Generated code supports async/await
- [ ] TypeScript: Generated code uses Zod schemas
- [ ] TypeScript: Generated code supports async/await
- [ ] Go: Generated code uses native structs
- [ ] Go: Generated code supports context.Context
- [ ] Ruby: Generated code uses Sorbet type annotations

---

## 12. Runtime Features

### 12.1 Client Registry
- [ ] As a developer, I can add clients dynamically at runtime
- [ ] As a developer, I can override client selection per function call with `baml_options={"client": "..."}`
- [ ] As a developer, I can create provider-specific clients programmatically

### 12.2 Dynamic Types (TypeBuilder)
- [ ] As a developer, I can add enum values at runtime
- [ ] As a developer, I can add class fields at runtime
- [ ] As a developer, I can use dynamic types with `@@dynamic` marked types
- [ ] As a developer, I can pass TypeBuilder to individual function calls
- [ ] As a developer, I can use `list_properties()` and `list_values()` on TypeBuilder
- [ ] As a developer, I can add aliases and descriptions to dynamic types

### 12.3 Collector (Observability)
- [ ] As a developer, I can attach a collector to function calls
- [ ] As a developer, I can access raw HTTP requests from the collector
- [ ] As a developer, I can access raw HTTP responses from the collector
- [ ] As a developer, I can access token usage metrics
- [ ] As a developer, I can access timing information
- [ ] As a developer, I can access complete call history (including retries)
- [ ] As a developer, I can access SSE (Server-Sent Events) response data

### 12.4 Tracing and Logging
- [ ] As a developer, I can use `on_log_event` callback for receiving logs
- [ ] As a developer, I can use tracing with tag support
- [ ] As a developer, I can access function logs and call metadata
- [ ] As a developer, I can use `on_tick` callbacks for streaming progress

### 12.5 Concurrency
- [ ] As a developer, I can execute multiple BAML functions concurrently
- [ ] As a developer, I can use async/await patterns

### 12.6 Cancellation
- [ ] As a developer, I can cancel function calls (AbortSignal in TypeScript)
- [ ] As a developer, I can use context cancellation (Go)
- [ ] As a developer, abort signals take priority over timeouts in error handling

---

## 13. Watch/Notification System

### 13.1 Watch Variables
- [ ] As a developer, I can use `watch let` variables for monitoring changes
- [ ] As a developer, I can use `.$watch.options()` for configuring watch behavior
- [ ] As a developer, I can pass filter functions to watch options
- [ ] As a developer, I can use LLM-based filter functions in watch

### 13.2 Event Listeners
- [ ] As a developer, I can register `on_var` listeners for variable changes
- [ ] As a developer, I can register `on_stream` listeners for stream events
- [ ] As a developer, I can register `on_block` listeners for block completion
- [ ] As a developer, I can register multiple listeners on the same variable

---

## 14. Error Handling

### 14.1 Error Hierarchy
- [ ] As a developer, I receive `BamlError` as the base exception type
- [ ] As a developer, I receive `BamlInvalidArgumentError` for invalid inputs
- [ ] As a developer, I receive `BamlClientError` for LLM client failures
- [ ] As a developer, I receive `BamlClientHttpError` for HTTP request failures (with status code)
- [ ] As a developer, I receive `BamlValidationError` for output validation failures
- [ ] As a developer, I receive `BamlClientFinishReasonError` for LLM finish reason issues
- [ ] As a developer, I receive `BamlAbortError` for cancellation/abortion
- [ ] As a developer, I receive `BamlTimeoutError` for timeout failures

### 14.2 Error Information
- [ ] As a developer, I can access `message` property on errors
- [ ] As a developer, I can access `prompt` property to see the original prompt
- [ ] As a developer, I can access `raw_output` property to see LLM response
- [ ] As a developer, I can access `detailed_message` with full error history (including fallback attempts)
- [ ] As a developer, I can access HTTP status codes on `BamlClientHttpError`

### 14.3 Validation Errors
- [ ] As a developer, `@assert` failures raise exceptions
- [ ] As a developer, `@check` failures return check results without raising
- [ ] As a developer, I can distinguish between assert and check behaviors
- [ ] As a developer, constraints behave differently during streaming vs final response

---

## 15. CLI Features

### 15.1 Project Initialization (`baml-cli init`)
- [ ] As a developer, I can initialize a new BAML project
- [ ] As a developer, I get a scaffolded `baml_src` directory
- [ ] As a developer, I can specify target language (Python, TypeScript, Go, Ruby)

### 15.2 Code Generation (`baml-cli generate`)
- [ ] As a developer, I can generate code from `.baml` files
- [ ] As a developer, I can specify output options
- [ ] As a developer, I get version checking for compatibility

### 15.3 Development Server (`baml-cli dev`)
- [ ] As a developer, I can run a dev server with hot-reload
- [ ] As a developer, I get auto-regeneration on file changes
- [ ] As a developer, I get file watching for rapid iteration

### 15.4 Testing (`baml-cli test`)
- [ ] As a developer, I can run BAML function tests
- [ ] As a developer, I can filter tests by function name pattern
- [ ] As a developer, I can filter tests by test name pattern
- [ ] As a developer, I can run tests in parallel with configurable concurrency
- [ ] As a developer, I can load environment variables from `.env` files
- [ ] As a developer, I get proper exit codes for CI/CD integration

### 15.5 HTTP Server (`baml-cli serve`)
- [ ] As a developer, I can serve BAML functions as HTTP endpoints
- [ ] As a developer, I get REST endpoints (`/call/:function_name`, `/stream/:function_name`)
- [ ] As a developer, I get Swagger UI documentation at `/docs`
- [ ] As a developer, I get OpenAPI spec generation
- [ ] As a developer, I can configure API key authentication

### 15.6 Formatting (`baml-cli fmt`)
- [ ] As a developer, I can format BAML files

---

## 16. IDE / LSP Features

### 16.1 Syntax Highlighting
- [ ] As a developer, I get syntax highlighting for BAML files
- [ ] As a developer, I get syntax highlighting for Jinja within prompts
- [ ] As a developer, I get proper highlighting for string literals (including block strings)
- [ ] As a developer, I get proper highlighting for comments
- [ ] As a developer, I get proper highlighting for keywords
- [ ] As a developer, I get proper highlighting for types
- [ ] As a developer, I get proper highlighting for attributes

### 16.2 Navigation
- [ ] As a developer, I can "Go to Definition" for types (classes, enums)
- [ ] As a developer, I can "Go to Definition" for functions
- [ ] As a developer, I can "Go to Definition" for clients
- [ ] As a developer, I can "Go to Definition" for template strings
- [ ] As a developer, I can "Go to Definition" for type aliases
- [ ] As a developer, I can "Go to Definition" from generated code back to BAML
- [ ] As a developer, I can "Find All References" for types
- [ ] As a developer, I can "Find All References" for functions
- [ ] As a developer, I can "Find All References" for clients
- [ ] As a developer, I can "Find All References" for template strings

### 16.3 Hover Information
- [ ] As a developer, I see type information on hover for variables
- [ ] As a developer, I see documentation on hover for types
- [ ] As a developer, I see function signatures on hover
- [ ] As a developer, I see client configuration on hover
- [ ] As a developer, I see enum values on hover
- [ ] As a developer, I see attribute documentation on hover

### 16.4 Symbols
- [ ] As a developer, I can view document symbols (outline) for BAML files
- [ ] As a developer, I can search workspace symbols across all BAML files

### 16.5 Call Hierarchy
- [ ] As a developer, I can view incoming calls (who calls this function)
- [ ] As a developer, I can view outgoing calls (what this function calls)
- [ ] As a developer, I can prepare call hierarchy at any function

### 16.6 Diagnostics
- [ ] As a developer, I see syntax errors in real-time
- [ ] As a developer, I see type errors in real-time
- [ ] As a developer, I see undefined reference errors
- [ ] As a developer, I see duplicate definition errors
- [ ] As a developer, I see attribute validation errors
- [ ] As a developer, I see Jinja syntax errors within prompts
- [ ] As a developer, I see recursive type cycle errors
- [ ] As a developer, I see environment variable missing warnings

### 16.7 Code Completions
- [ ] As a developer, I get completions for type names
- [ ] As a developer, I get completions for function names
- [ ] As a developer, I get completions for client names
- [ ] As a developer, I get completions for template string names
- [ ] As a developer, I get completions for field names
- [ ] As a developer, I get completions for enum values
- [ ] As a developer, I get completions for attributes
- [ ] As a developer, I get completions for attribute parameters
- [ ] As a developer, I get completions for Jinja variables in prompts
- [ ] As a developer, I get completions for Jinja filters
- [ ] As a developer, I get completions for provider options

### 16.8 Code Actions
- [ ] As a developer, I can quick-fix common errors
- [ ] As a developer, I can auto-import missing types

### 16.9 Formatting
- [ ] As a developer, I can format BAML documents
- [ ] As a developer, I can format on save

### 16.10 Code Lens
- [ ] As a developer, I see code lens indicators for functions (e.g., "Run Test")
- [ ] As a developer, I can click code lens to execute actions

### 16.11 Snippets
- [ ] As a developer, I can use snippets to scaffold common constructs
- [ ] As a developer, I get snippets for classes, enums, functions, clients, tests

---

## 17. Playground (IDE Feature)

### 17.1 Test Execution
- [ ] As a developer, I can run tests from the playground
- [ ] As a developer, I can run tests in parallel
- [ ] As a developer, I can see test results in the playground

### 17.2 Prompt Preview
- [ ] As a developer, I can preview the fully rendered prompt
- [ ] As a developer, I can see how `ctx.output_format` renders
- [ ] As a developer, I can see role-based message structure

### 17.3 Debugging
- [ ] As a developer, I can view raw cURL commands for requests
- [ ] As a developer, I can see request/response details
- [ ] As a developer, I can see token usage

### 17.4 Environment Configuration
- [ ] As a developer, I can configure environment variables in playground
- [ ] As a developer, I can switch between different environments

---

## 18. Framework Integrations

### 18.1 React/Next.js
- [ ] As a developer, I get automatic server action generation (Next.js 15+)
- [ ] As a developer, I get generated React hooks (`use{FunctionName}`)
- [ ] As a developer, I get streaming support with React Server Components
- [ ] As a developer, I get built-in error and loading states

### 18.2 REST API
- [ ] As a developer, I can generate OpenAPI spec from BAML functions
- [ ] As a developer, I can serve functions as REST endpoints

---

## 19. Compiler/Validation

### 19.1 Semantic Analysis
- [ ] As a compiler, I validate all type references resolve correctly
- [ ] As a compiler, I validate function signatures are well-formed
- [ ] As a compiler, I validate client configurations are valid
- [ ] As a compiler, I validate attribute syntax and usage
- [ ] As a compiler, I validate Jinja template syntax
- [ ] As a compiler, I validate circular/recursive type dependencies are handled correctly
- [ ] As a compiler, I validate constraint expressions are valid

### 19.2 Type Checking
- [ ] As a compiler, I check assignment compatibility
- [ ] As a compiler, I check function argument types
- [ ] As a compiler, I check return type compatibility
- [ ] As a compiler, I check optional type usage
- [ ] As a compiler, I check union type validity
- [ ] As a compiler, I check literal type constraints
- [ ] As a compiler, I check recursive type bounds

### 19.3 Error Messages
- [ ] As a compiler, I provide clear error messages with line/column info
- [ ] As a compiler, I provide suggestions for common mistakes
- [ ] As a compiler, I provide context for where errors occurred

---

## 20. Environment Variables

### 20.1 Definition
- [ ] As a developer, I can reference env vars with `env.VAR_NAME` syntax
- [ ] As a developer, I get lazy evaluation of env vars (only resolved at runtime)
- [ ] As a developer, I can set env vars to `null` explicitly

### 20.2 Loading
- [ ] As a developer, I can load env vars from `.env` files
- [ ] As a developer, I can override env vars per test
- [ ] As a developer, I get errors when required env vars are missing at runtime

---

## 21. Output Format Generation

### 21.1 Schema Generation
- [ ] As a runtime, I generate JSON schema instructions from return types
- [ ] As a runtime, I handle nested object schemas
- [ ] As a runtime, I handle array schemas
- [ ] As a runtime, I handle union type schemas
- [ ] As a runtime, I handle optional field schemas
- [ ] As a runtime, I handle recursive type schemas
- [ ] As a runtime, I include field descriptions from attributes
- [ ] As a runtime, I include field aliases from attributes
- [ ] As a runtime, I support alternative format styles (e.g., `map_style='angle'`)

### 21.2 Response Parsing
- [ ] As a runtime, I parse JSON responses from LLMs
- [ ] As a runtime, I handle malformed JSON with error correction
- [ ] As a runtime, I validate parsed responses against expected types
- [ ] As a runtime, I execute `@assert` validations on parsed responses
- [ ] As a runtime, I execute `@check` validations and return results
- [ ] As a runtime, I handle union type discrimination (including via literal properties)

---

## 22. Streaming Implementation

### 22.1 Partial Type Generation
- [ ] As a code generator, I create partial types for streaming responses
- [ ] As a code generator, I handle optional fields in partial types
- [ ] As a code generator, I handle nested objects in partial types

### 22.2 Stream Processing
- [ ] As a runtime, I incrementally parse streaming JSON
- [ ] As a runtime, I yield partial objects as they become available
- [ ] As a runtime, I provide final complete response after stream ends
- [ ] As a runtime, I handle `@stream.done` markers during streaming
- [ ] As a runtime, I handle constraint validation differently during streaming

---

## 23. Documentation Generation

### 23.1 OpenAPI Generation
- [ ] As a developer, I can generate OpenAPI specs from BAML functions
- [ ] As a developer, I get proper request/response schemas in OpenAPI
- [ ] As a developer, I get Swagger UI for interactive documentation

---

## 24. Backwards Compatibility

### 24.1 Version Management
- [ ] As a developer, I can specify BAML version requirements
- [ ] As a developer, I get warnings for deprecated features
- [ ] As a developer, I get migration guidance for breaking changes

---

## Summary Statistics

- **Total Categories**: 24
- **Total User Stories**: ~350+

---

## Priority Tiers

### Tier 1 - MVP (Core Functionality)
- Language Core (Parser & AST) - sections 1.1-1.6
- Type Definitions (Classes, Enums, basic Type Aliases)
- Basic Functions (with prompt block)
- Basic Jinja Templating (variables, loops, conditionals)
- LLM Clients (at least OpenAI, Anthropic)
- Code Generation (at least one language)
- Basic CLI (init, generate)
- Basic Diagnostics (syntax/type errors)
- Basic Error Handling

### Tier 2 - Essential DX
- Full Attribute System (@alias, @description, @assert, @check)
- Template Strings
- Test Blocks
- LSP Navigation (Go to Definition, Find References)
- LSP Hover Information
- LSP Code Completions
- Client Strategies (Retry, Fallback)
- CLI Testing (`baml-cli test`)
- Full Error Hierarchy
- Environment Variables

### Tier 3 - Advanced Features
- Streaming Support (including partial types)
- Dynamic Types (TypeBuilder, @@dynamic)
- Collector (Observability)
- Playground
- All Provider Support
- Multimodal Types (image, audio, video, pdf)
- Full LSP (Call Hierarchy, Code Actions, Symbols)
- CLI Server (`baml-cli serve`)
- Recursive Types
- Union Type Discrimination

### Tier 4 - Power Features
- Pure Expression Functions (no LLM)
- Built-in Functions (baml.fetch_as, etc.)
- Watch/Notification System
- Streaming Attributes (@stream.done, etc.)
- Block-level Cross-field Validation
- Extended Thinking (Anthropic, Google)
- Prompt Caching (metadata)
- All Timeout Configurations
- Finish Reason Allow Lists

### Tier 5 - Polish & Integrations
- OpenAPI Generation
- React Hooks Generation
- Framework Integrations (Next.js)
- Round-Robin Strategy
- Full Code Lens Features
- Snippets
- Advanced Media URL Handlers
- Tracing with Tags
