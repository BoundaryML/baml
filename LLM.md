# BAML (Boundary AI Markup Language) - Complete API Reference

## Overview

BAML is a domain-specific language for building reliable AI workflows and agents. It turns prompt engineering into schema engineering, focusing on structured inputs and outputs for more reliable LLM interactions.

**Key Features:**
- Type-safe LLM function calls
- Multi-language client generation (Python, TypeScript, Ruby, Go)
- Streaming support with partial results
- Built-in retry policies and fallback strategies
- Comprehensive IDE tooling and testing framework
- Support for 100+ LLM providers

## Installation

### Python
```bash
pip install baml-py
```

### TypeScript/JavaScript
```bash
npm install @boundaryml/baml
# or
yarn add @boundaryml/baml
# or
pnpm add @boundaryml/baml
```

### CLI
```bash
npm install -g @boundaryml/baml
# or via pip
pip install baml-py
```

## Quick Start

### 1. Initialize Project
```bash
baml init --client-type python
# or
baml init --client-type typescript
```

### 2. Define BAML Function
```baml
// baml_src/main.baml
function ExtractResume(resume_text: string) -> Resume {
  client GPT4o
  prompt #"
    Extract resume information from:
    {{ resume_text }}
    
    {{ ctx.output_format }}
  "#
}

class Resume {
  name string
  email string
  skills string[]
  experience WorkExperience[]
}

class WorkExperience {
  company string
  position string
  duration string
}

client<llm> GPT4o {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}
```

### 3. Generate Client
```bash
baml generate
```

### 4. Use Generated Client

**Python:**
```python
from baml_client import b

result = await b.ExtractResume(resume_text="John Doe...")
print(f"Name: {result.name}")
print(f"Skills: {result.skills}")
```

**TypeScript:**
```typescript
import { b } from './baml_client'

const result = await b.ExtractResume("John Doe...")
console.log(`Name: ${result.name}`)
console.log(`Skills: ${result.skills}`)
```


# BAML Language Syntax Reference

## Function Definitions

### Basic Function Syntax
```baml
function FunctionName(param1: Type1, param2: Type2) -> ReturnType {
  client ClientName
  prompt #"
    Your prompt template here
    {{ param1 }} {{ param2 }}
    {{ ctx.output_format }}
  "#
}
```

### Function with Multiple Clients
```baml
function ExtractData(text: string) -> DataType {
  client GPT4o | Claude | Gemini  // Try in order
  prompt #"
    Extract structured data from: {{ text }}
    {{ ctx.output_format }}
  "#
}
```

### Streaming Function
```baml
function StreamingChat(message: string) -> string {
  client GPT4o
  prompt #"
    {{ _.role("user") }}
    {{ message }}
  "#
}
```

## Type System

### Classes (Structured Types)
```baml
class Person {
  name string
  age int
  email string?
  active bool
  score float
}

// With descriptions and aliases
class BookOrder {
  orderId string @description(#"
    The unique identifier for the book order
  "#)
  title string @alias("book_title")
  quantity int @check(positive, {{ this > 0 }})
  price float
}
```

### Enums
```baml
enum Status {
  PENDING @alias("pending")
  COMPLETED @description("Task completed successfully")
  FAILED
  
  @@alias("STATUS_ENUM")  // Class-level alias
}
```

### Union Types
```baml
class FlexibleType {
  value string | int | bool
  items (string[] | int[])
  optional_union (string | null)?
}
```

### Dynamic Types
```baml
class DynamicClass {
  @@dynamic  // Schema inferred at runtime
}

enum DynamicEnum {
  @@dynamic
}
```

### Recursive Types
```baml
class TreeNode {
  value string
  children TreeNode[]
}
```

## Client Configurations

### OpenAI Client
```baml
client<llm> GPT4 {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
    max_tokens 1000
    temperature 0.7
  }
}
```

### Azure OpenAI Client
```baml
client<llm> AzureGPT {
  provider azure-openai
  options {
    resource_name "your-resource"
    deployment_id "gpt-4-deployment"
    api_version "2024-02-01"
    api_key env.AZURE_OPENAI_API_KEY
  }
}
```

### Anthropic Client
```baml
client<llm> Claude {
  provider anthropic
  options {
    model "claude-3-5-sonnet-20241022"
    api_key env.ANTHROPIC_API_KEY
    max_tokens 4096
  }
}
```

### Google AI Client
```baml
client<llm> Gemini {
  provider google-ai
  options {
    model "gemini-2.5-flash"
    api_key env.GOOGLE_API_KEY
  }
}
```

### AWS Bedrock Client
```baml
client<llm> BedrockClaude {
  provider aws-bedrock
  options {
    model "anthropic.claude-3-5-sonnet-20240620-v1:0"
    region "us-east-1"
    access_key_id env.AWS_ACCESS_KEY_ID
    secret_access_key env.AWS_SECRET_ACCESS_KEY
  }
}
```

### Ollama Client
```baml
client<llm> LocalLlama {
  provider ollama
  options {
    model "llama3.1"
    base_url "http://localhost:11434"  // Optional
  }
}
```

## Resilience Strategies

### Retry Policies
```baml
retry_policy ExponentialBackoff {
  max_retries 3
  strategy {
    type exponential_backoff
    delay_ms 1000
    multiplier 2.0
    max_delay_ms 10000
  }
}

retry_policy ConstantDelay {
  max_retries 5
  strategy {
    type constant_delay
    delay_ms 2000
  }
}
```

### Fallback Strategy
```baml
client<llm> ResilientClient {
  provider baml-fallback
  options {
    strategy [
      GPT4Turbo,    // Try first
      Claude,       // Fallback to second
      Gemini        // Final fallback
    ]
  }
}
```

### Round Robin Strategy
```baml
client<llm> LoadBalanced {
  provider baml-round-robin
  options {
    start 0
    strategy [
      GPT4,
      Claude,
      Gemini
    ]
  }
}
```

## Prompt Templates

### Basic Templating
```baml
function BasicPrompt(name: string, age: int) -> string {
  client GPT4o
  prompt #"
    Hello {{ name }}, you are {{ age }} years old.
    {{ ctx.output_format }}
  "#
}
```

### Chat Roles
```baml
function ChatPrompt(user_message: string) -> string {
  client GPT4o
  prompt #"
    {{ _.role("system") }}
    You are a helpful assistant.
    
    {{ _.role("user") }}
    {{ user_message }}
  "#
}
```

### Conditional Logic
```baml
function ConditionalPrompt(user: User) -> string {
  client GPT4o
  prompt #"
    {% if user.is_premium %}
      Welcome, premium user {{ user.name }}!
    {% else %}
      Hello {{ user.name }}, consider upgrading to premium.
    {% endif %}
    
    {{ ctx.output_format }}
  "#
}
```

### Loops
```baml
function ProcessMessages(messages: Message[]) -> Summary {
  client GPT4o
  prompt #"
    Summarize this conversation:
    
    {% for msg in messages %}
    {{ _.role(msg.role) }}
    {{ msg.content }}
    {% endfor %}
    
    {{ ctx.output_format }}
  "#
}
```

### Template Strings
```baml
template_string FormatMessages(messages: Message[]) #"
  {% for m in messages %}
    {{ _.role(m.role) }}
    {{ m.content }}
  {% endfor %}
"#

function UseTemplate(messages: Message[]) -> string {
  client GPT4o
  prompt #"
    Analyze this conversation:
    {{ FormatMessages(messages) }}
    {{ ctx.output_format }}
  "#
}
```

## Constraints and Validation

### Field-level Constraints
```baml
class ValidatedPerson {
  age int @check(adult, {{ this >= 18 }})
  email string @check(valid_email, {{ this | regex_match("^[^@]+@[^@]+\.[^@]+$") }})
  name string @check(not_empty, {{ this | length > 0 }})
}
```

### Multiple Constraints
```baml
class StrictValidation {
  score int @check(min_score, {{ this >= 0 }})
           @check(max_score, {{ this <= 100 }})
           @check(even_score, {{ this % 2 == 0 }})
}
```

### Block-level Constraints
```baml
class CrossFieldValidation {
  start_date string
  end_date string
  
  @@check(date_order, {{ this.end_date > this.start_date }})
}
```

### Function Parameter Constraints
```baml
function ValidatedFunction(
  text: string @assert(min_length, {{ this | length > 10 }})
) -> ProcessedText {
  client GPT4o
  prompt #"
    Process: {{ text }}
    {{ ctx.output_format }}
  "#
}
```


# Python Client API Reference

## Installation and Import
```python
pip install baml-py

# Main imports
from baml_client import b, types, stream_types, tracing, config
from baml_py import BamlError, BamlClientError, BamlValidationError
from baml_py import Image, Audio, ClientRegistry, Collector
```

## Core Client Classes

### BamlAsyncClient
```python
# Pre-instantiated async client
from baml_client import b

# Function calls
result = await b.MyFunction(param="value")

# With options
result = await b.with_options(
    collector=Collector(),
    env={"API_KEY": "value"}
).MyFunction(param="value")
```

### BamlSyncClient
```python
# Synchronous client
from baml_client.sync_client import b as sync_b

result = sync_b.MyFunction(param="value")
```

## Function Calling

### Basic Function Calls
```python
# Async
result = await b.ExtractResume(resume_text="John Doe...")

# Sync
result = sync_b.ExtractResume(resume_text="John Doe...")

# With custom options
result = await b.with_options(
    client_registry=ClientRegistry(),
    collector=[collector1, collector2],
    env={"MODEL": "gpt-4"}
).ExtractResume(resume_text="...")
```

### Configuration Options
```python
from baml_py import ClientRegistry, Collector, TypeBuilder

# BamlCallOptions
options = {
    "tb": TypeBuilder(),                    # Custom type builder
    "client_registry": ClientRegistry(),    # Custom LLM clients
    "collector": [collector1, collector2],  # Observability
    "env": {"API_KEY": "value"}            # Environment variables
}

result = await b.with_options(**options).MyFunction(param="value")
```

## Streaming API

### Basic Streaming
```python
# Create stream
stream = b.stream.GenerateStory(prompt="Write a story about...")

# Iterate over partial results
async for partial_result in stream:
    print(f"Partial: {partial_result}")
    
# Get final result
final_result = await stream.get_final_response()
```

### Stream Event Handling
```python
stream = b.stream.ExtractData(text="...")

async for event in stream:
    if hasattr(event, 'state'):
        if event.state == "Incomplete":
            print(f"Partial: {event.value}")
        elif event.state == "Complete":
            print(f"Final: {event.value}")
            break
    else:
        print(f"Chunk: {event}")
```

### Synchronous Streaming
```python
stream = sync_b.stream.MyFunction(input="data")

for partial_result in stream:
    print(f"Partial: {partial_result}")
    
final_result = stream.get_final_response()
```

## Type System Integration

### Generated Types
```python
from baml_client import types

# Use generated Pydantic models
person = types.Person(
    name="John",
    age=30,
    email="john@example.com"
)

result = await b.ProcessPerson(person=person)
```

### Checked Types (Validation)
```python
from baml_client import types

# Checked types include validation results
result: types.Checked[int, Literal['gt_ten']] = await b.ValidateNumber(num=15)

print(f"Value: {result.value}")
print(f"Checks: {result.checks}")
print(f"All passed: {types.all_succeeded(result.checks)}")

# Access individual check results
if result.checks['gt_ten'].status == 'succeeded':
    print("Validation passed")
```

### Media Types
```python
from baml_py import Image, Audio

# Image handling
image = Image.from_url("https://example.com/image.jpg")
# or
image = Image.from_base64("image/jpeg", base64_data)
# or
image = Image.from_path("./image.jpg")

result = await b.AnalyzeImage(image=image)

# Audio handling
audio = Audio.from_url("https://example.com/audio.mp3")
result = await b.TranscribeAudio(audio=audio)
```

## Error Handling

### Exception Types
```python
from baml_py import (
    BamlError,                    # Base exception
    BamlClientError,              # Client-side errors
    BamlClientHttpError,          # HTTP errors
    BamlValidationError,          # Type validation errors
    BamlClientFinishReasonError,  # LLM finish reason errors
    BamlInvalidArgumentError      # Invalid arguments
)
```

### Error Handling Pattern
```python
try:
    result = await b.MyFunction(input="data")
except BamlValidationError as e:
    print(f"Validation failed: {e}")
    print(f"Raw output: {e.raw_output}")
except BamlClientHttpError as e:
    print(f"HTTP error {e.status_code}: {e}")
except BamlClientError as e:
    print(f"Client error: {e}")
except BamlError as e:
    print(f"General BAML error: {e}")
```

## HTTP Request Building

### Request Builder
```python
# Build HTTP request without executing
http_request = await b.request.MyFunction(input="data")

print(f"URL: {http_request.url}")
print(f"Headers: {http_request.headers}")
print(f"Body: {http_request.body}")
```

### Streaming Request Builder
```python
stream_request = await b.stream_request.MyStreamingFunction(input="data")
```

## Response Parsing

### Parse LLM Response
```python
# Parse raw LLM response
llm_response = "{'name': 'John', 'age': 30}"
parsed = await b.parse.ExtractPerson(llm_response)
print(f"Parsed: {parsed}")
```

### Parse Streaming Response
```python
stream_parser = b.parse_stream.ExtractData(llm_response_stream)
async for partial in stream_parser:
    print(f"Partial parse: {partial}")
```

## Tracing and Observability

### Function Tracing
```python
from baml_client import tracing

# Trace any function
@tracing.trace
async def my_function(data: str):
    result = await b.ProcessData(data=data)
    return result

# Set trace tags
tracing.set_tags(user_id="123", session_id="abc")
```

### Custom Collectors
```python
from baml_py import Collector

class MyCollector(Collector):
    def on_function_log(self, log):
        print(f"Function: {log.function_name}")
        print(f"Duration: {log.timing.duration_ms}ms")
        print(f"Cost: ${log.cost}")

collector = MyCollector()
client = b.with_options(collector=collector)
```

## Configuration

### Environment Variables
```python
import os

# Set via environment
os.environ["BAML_LOG"] = "DEBUG"
os.environ["OPENAI_API_KEY"] = "sk-..."

# Or via client options
client = b.with_options(env={
    "BAML_LOG": "INFO",
    "MODEL_NAME": "gpt-4"
})
```

### Logging Configuration
```python
# Use environment variables
os.environ["BAML_LOG"] = "DEBUG"
os.environ["BAML_LOG_JSON_MODE"] = "true"
os.environ["BAML_LOG_MAX_CHUNK_LENGTH"] = "1000"
```


# TypeScript/JavaScript Client API Reference

## Installation and Import
```bash
npm install @boundaryml/baml
# or
yarn add @boundaryml/baml
# or
pnpm add @boundaryml/baml
```

```typescript
// Main client import
import { b } from './baml_client'
// or
import { b } from './baml_client/async_client'

// Sync client
import { b as syncB } from './baml_client/sync_client'

// Type imports
import type * as types from './baml_client/types'
import type { partial_types } from './baml_client/partial_types'

// Core BAML types
import type { 
  Image, 
  Audio, 
  BamlStream, 
  Collector,
  ClientRegistry,
  TypeBuilder 
} from '@boundaryml/baml'

// Error types
import { 
  BamlClientHttpError,
  BamlValidationError, 
  BamlClientFinishReasonError 
} from '@boundaryml/baml'
```

## Core Client Classes

### BamlAsyncClient
```typescript
class BamlAsyncClient {
  // Function calls
  async YourFunction(param: string): Promise<ReturnType>
  
  // Streaming
  get stream: BamlStreamClient
  
  // HTTP request building
  get request: AsyncHttpRequest
  get streamRequest: AsyncHttpStreamRequest
  
  // Response parsing
  get parse: LlmResponseParser
  get parseStream: LlmStreamParser
  
  // Configuration
  withOptions(options: BamlCallOptions): BamlAsyncClient
}
```

### BamlSyncClient
```typescript
class BamlSyncClient {
  // Synchronous function calls
  YourFunction(param: string): ReturnType
  
  // HTTP request building
  get request: HttpRequest
  get streamRequest: HttpStreamRequest
  
  // Response parsing
  get parse: LlmResponseParser
  get parseStream: LlmStreamParser
  
  // Configuration
  withOptions(options: BamlCallOptions): BamlSyncClient
}
```

## Function Calling

### Basic Function Calls
```typescript
// Async
const result = await b.ExtractResume(resumeText)

// Sync
const result = syncB.ExtractResume(resumeText)
```

### With Options
```typescript
type BamlCallOptions = {
  tb?: TypeBuilder           // Type builder for dynamic types
  clientRegistry?: ClientRegistry  // Custom client configuration
  collector?: Collector | Collector[]  // Logging/tracing
  env?: Record<string, string | undefined>  // Environment variables
}

// Using options
const result = await b.withOptions({
  collector: new Collector('my-collector'),
  env: { OPENAI_API_KEY: 'custom-key' }
}).ExtractResume(resumeText)
```

## Streaming API

### Basic Streaming
```typescript
// Create stream
const stream = b.stream.ExtractResume(resumeText)

// Iterate over partial results
for await (const partial of stream) {
  console.log('Partial:', partial)
}

// Get final result
const final = await stream.getFinalResponse()
```

### BamlStream Class
```typescript
class BamlStream<PartialOutputType, FinalOutputType> {
  // Async iteration
  [Symbol.asyncIterator](): AsyncIterableIterator<PartialOutputType>
  
  // Get final response
  getFinalResponse(): Promise<FinalOutputType>
  
  // Convert to Next.js compatible stream
  toStreamable(): ReadableStream<Uint8Array>
}
```

### Next.js/React Integration
```typescript
// Server Action
'use server'
import { b } from './baml_client'

export async function streamingAction(input: string) {
  return b.stream.ExtractResume(input).toStreamable()
}

// Generated React server streaming functions
import { ExtractResume } from './baml_client/react/server_streaming'

export const streamingServerAction = async (input: string) => {
  return ExtractResume(input) // Returns ReadableStream<Uint8Array>
}
```

## Type System Integration

### Generated Types
```typescript
// Your BAML classes become TypeScript interfaces
interface Resume {
  name: string
  email: string
  skills: string[]
  experience: WorkExperience[]
}

interface WorkExperience {
  company: string
  position: string
  duration: string
}

// Enums become TypeScript enums
enum Category {
  Technical = "Technical",
  Marketing = "Marketing",
  Sales = "Sales"
}
```

### Partial Types for Streaming
```typescript
// Partial types for streaming responses
namespace partial_types {
  interface Resume {
    name?: string | null
    email?: string | null
    skills?: string[] | null
    experience?: WorkExperience[] | null
  }
}
```

### Checked Types (Constraints)
```typescript
interface Checked<T, CheckName extends string = string> {
  value: T
  checks: Record<CheckName, Check>
}

interface Check {
  name: string
  expr: string
  status: "succeeded" | "failed"
}

// Usage
const result: Checked<number, "gt_ten"> = await b.ValidateAge(25)
console.log(result.value) // 25
console.log(result.checks.gt_ten.status) // "succeeded"
```

## Error Handling

### Error Types
```typescript
// HTTP errors
class BamlClientHttpError extends Error {
  client_name: string
  status_code: number
}

// Validation errors
class BamlValidationError extends Error {
  prompt: string
  raw_output: string
}

// Finish reason errors
class BamlClientFinishReasonError extends Error {
  prompt: string
  raw_output: string
  finish_reason?: string
}
```

### Error Handling Pattern
```typescript
import { 
  BamlClientHttpError,
  BamlValidationError,
  BamlClientFinishReasonError,
  isBamlError 
} from '@boundaryml/baml'

try {
  const result = await b.ExtractResume(resumeText)
} catch (error) {
  if (isBamlError(error)) {
    if (error instanceof BamlClientHttpError) {
      console.log(`HTTP Error ${error.status_code}: ${error.message}`)
    } else if (error instanceof BamlValidationError) {
      console.log(`Validation Error: ${error.message}`)
      console.log(`Raw output: ${error.raw_output}`)
    } else if (error instanceof BamlClientFinishReasonError) {
      console.log(`Finish Reason: ${error.finish_reason}`)
    }
  }
}
```

## Media Support

### Images
```typescript
import { Image } from '@boundaryml/baml'

// From URL
const image = Image.from_url('https://example.com/image.jpg')

// From base64
const image = Image.from_base64('data:image/jpeg;base64,...')

// From file path (Node.js only)
const image = Image.from_path('./image.jpg')

// Use in function
const result = await b.AnalyzeImage(image)
```

### Audio
```typescript
import { Audio } from '@boundaryml/baml'

// From URL
const audio = Audio.from_url('https://example.com/audio.mp3')

// From base64
const audio = Audio.from_base64('data:audio/mpeg;base64,...')

// From file path (Node.js only)
const audio = Audio.from_path('./audio.mp3')

// Use in function
const result = await b.TranscribeAudio(audio)
```

## HTTP Request Building

### Request Objects
```typescript
// Build request without sending
const request = await b.request.ExtractResume(resumeText)
console.log(request.url)
console.log(request.headers)
console.log(request.body)

// Build streaming request
const streamRequest = await b.streamRequest.ExtractResume(resumeText)
```

### HTTPRequest Type
```typescript
interface HTTPRequest {
  url: string
  headers: Record<string, string>
  body: any
}
```

## Response Parsing

### Parse Raw LLM Responses
```typescript
// Parse non-streaming response
const parsed = b.parse.ExtractResume(rawLlmResponse)

// Parse streaming response
const stream = b.parseStream.ExtractResume(rawStreamingResponse)
for await (const partial of stream) {
  console.log(partial)
}
```

## Configuration

### Client Registry
```typescript
// Custom client configuration
const clientRegistry = new ClientRegistry()
const result = await b.withOptions({ clientRegistry }).ExtractResume(text)
```

### Collectors (Logging/Tracing)
```typescript
// Create collector for logging
const collector = new Collector('my-collector')

// Use collector
await b.withOptions({ collector }).ExtractResume(text)

// Access logs
const logs = collector.logs
const lastLog = collector.last
```

### Environment Variables
```typescript
// Override environment variables
const result = await b.withOptions({
  env: {
    OPENAI_API_KEY: 'custom-key',
    ANTHROPIC_API_KEY: 'another-key'
  }
}).ExtractResume(text)
```


# CLI and Development Tools

## CLI Installation
```bash
npm install -g @boundaryml/baml
# or
pip install baml-py
```

## Core CLI Commands

### `baml init` - Initialize Project
```bash
baml init [OPTIONS]

# Options:
--dest <PATH>              # Project directory (default: current)
--client-type <TYPE>       # Client type to generate
  # python/pydantic - Python with Pydantic models
  # typescript - TypeScript client
  # ruby/sorbet - Ruby with Sorbet types
  # rest/openapi - REST API with OpenAPI spec
  # go - Go client

# Examples:
baml init --client-type typescript --dest ./my-baml-project
baml init --client-type python/pydantic
```

### `baml generate` - Generate Client Code
```bash
baml generate [OPTIONS]

# Options:
--from <PATH>              # Path to baml_src directory (default: ./baml_src)
--no-version-check         # Skip version mismatch checking

# Examples:
baml generate
baml generate --from ./custom-baml-src
```

### `baml test` - Run Tests
```bash
baml test [OPTIONS]

# Options:
--from <PATH>              # Path to baml_src directory
--list                     # Only list selected tests
-i, --include <PATTERN>    # Include specific functions/tests
-x, --exclude <PATTERN>    # Exclude specific functions/tests
--parallel <N>             # Number of parallel tests (default: 10)
--pass-if-no-tests         # Pass if no tests selected
--require-human-eval       # Fail if tests need human evaluation
--dotenv                   # Load .env file (default: true)
--dotenv-path <PATH>       # Custom .env file path

# Test Filter Examples:
baml test -i "Extract*"                    # Functions starting with "Extract"
baml test -i "GetWeather::test_sunny_day"  # Specific test in function
baml test -i "GetWeather::"                # All tests in GetWeather
baml test -i "::basic_test"                # Test named "basic_test" in any function
baml test -x "*flaky*"                     # Exclude flaky tests
```

### `baml dev` - Development Server
```bash
baml dev [OPTIONS]

# Options:
--from <PATH>              # Path to baml_src directory
--port <PORT>              # Port to expose (default: 2024)

# Features:
# - File watching with auto-reload
# - Hot reloading of BAML runtime
# - Automatic client code generation
# - Integrated playground

# Example:
baml dev --port 3000
```

### `baml serve` - Production Server
```bash
baml serve [OPTIONS]

# Options:
--from <PATH>              # Path to baml_src directory
--port <PORT>              # Port to expose (default: 2024)
--no-version-check         # Skip version checking

# Authentication:
# Set BAML_PASSWORD environment variable
# Supports Basic Auth and x-baml-api-key header
# Recommended format: sk-baml-...

# Example:
BAML_PASSWORD=sk-baml-secret baml serve --port 8080
```

### `baml fmt` - Format Code
```bash
baml fmt [OPTIONS] [FILES...]

# Options:
--from <PATH>              # Path to baml_src directory
-n, --dry-run              # Preview changes without writing
[FILES...]                 # Specific files to format

# Examples:
baml fmt --dry-run         # Preview formatting changes
baml fmt main.baml         # Format specific file
baml fmt                   # Format all .baml files
```

### `baml lsp` - Language Server
```bash
baml lsp [OPTIONS]

# Options:
--port <PORT>              # Port for language server (default: 2025)

# Used by IDE extensions for:
# - Real-time diagnostics
# - Code completion
# - Hover information
# - Go to definition
```

## VS Code Extension

### Installation
Search for "BAML" in VS Code Extensions marketplace or install from:
- Extension ID: `boundaryml.baml`

### Features
1. **Syntax Highlighting** - Full BAML language support
2. **Language Server Integration** - Real-time validation
3. **Code Completion** - Intelligent autocomplete
4. **Integrated Playground** - Test functions in VS Code
5. **Error Diagnostics** - Inline error reporting

### Commands
Available through Command Palette (`Ctrl+Shift+P`):

#### `BAML: Open Playground`
- Opens integrated playground panel
- Test functions interactively
- View real-time results

#### `BAML: Run Test`
- Run specific test cases
- Integrated result visualization

### Status Bar
- Shows BAML extension status
- Color-coded indicators:
  - 🟢 Green: All validations pass
  - 🟡 Yellow: Warnings present
  - 🔴 Red: Errors detected

### Configuration
```json
// VS Code settings.json
{
  "baml.bamlPanelOpen": true,
  "baml.enableDiagnostics": true
}
```

## Playground Features

### Interactive Testing Environment
- **Function Selection** - Choose any BAML function
- **Parameter Input** - Interactive form for function parameters
- **Real-time Execution** - Run functions with live results
- **Response Visualization** - Formatted output display
- **Error Debugging** - Detailed error information
- **Test Case Management** - Save and manage test cases

### Playground Server
- Runs on configurable port (default: 2024)
- WebSocket support for real-time updates
- CORS enabled for development
- Proxy support for external API calls

## Development Workflow

### Typical Development Flow
1. **Initialize Project**
   ```bash
   baml init --client-type typescript
   ```

2. **Start Development Server**
   ```bash
   baml dev --port 2024
   ```

3. **Write BAML Functions** in `baml_src/` directory

4. **Test Functions**
   ```bash
   baml test -i "MyFunction::"
   ```

5. **Generate Client Code**
   ```bash
   baml generate
   ```

6. **Use Generated Client** in your application

### File Watching
The development server automatically:
- Watches `baml_src/` directory for changes
- Reloads BAML runtime on modifications
- Regenerates client code
- Updates playground in real-time
- Shows compilation errors immediately

### Testing Strategy
- Use descriptive test names with patterns
- Leverage parallel testing for performance
- Implement human evaluation for subjective tests
- Use environment variables for API keys
- Organize tests by function and scenario

## Language Server Protocol (LSP)

### Features
- **Diagnostics** - Real-time error checking
- **Hover Information** - Documentation on hover
- **Go to Definition** - Navigate to definitions
- **Code Completion** - Context-aware suggestions
- **Formatting** - Automatic code formatting
- **Workspace Support** - Multiple folders

### Integration
- VS Code extension automatically starts LSP
- Communicates via JSON-RPC protocol
- Real-time validation and error reporting
- Supports incremental updates

## Code Generation

### Generator Configuration
```baml
generator my_client {
  output_type "typescript"
  output_dir "../baml_client"
  version "0.x.x"
  default_client_mode "async"
}
```

### Supported Output Types
- `python/pydantic` - Python with Pydantic models
- `typescript` - TypeScript/JavaScript client
- `ruby/sorbet` - Ruby with Sorbet annotations
- `rest/openapi` - OpenAPI specification
- `go` - Go client library

### Generation Process
1. Parse BAML source files
2. Validate syntax and semantics
3. Generate client code based on configuration
4. Run post-generation commands (if specified)
5. Update IDE with new definitions

## Environment Variables

### Common Environment Variables
```bash
# BAML Configuration
BAML_LOG=DEBUG                    # Log level (DEBUG, INFO, WARN, ERROR)
BAML_LOG_JSON_MODE=true          # JSON formatted logs
BAML_LOG_MAX_CHUNK_LENGTH=1000   # Max log chunk size
BAML_PASSWORD=sk-baml-secret     # Server authentication

# LLM Provider API Keys
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
GOOGLE_API_KEY=...
AWS_ACCESS_KEY_ID=...
AWS_SECRET_ACCESS_KEY=...
```

### Loading Environment Variables
```bash
# From .env file (automatically loaded)
baml test --dotenv

# From custom .env file
baml test --dotenv-path ./custom.env

# Disable .env loading
baml test --no-dotenv
```

## Exit Codes
- `0` - Success
- `1` - Human evaluation required (tests)
- `2` - Test failure
- `3` - Test cancelled/interrupted
- `4` - Internal error or invalid arguments
- `5` - No tests found


# LLM Provider Integrations

## Supported Providers

BAML supports 100+ LLM providers through the following provider types:

### Core Providers
- **openai** - OpenAI GPT models
- **azure-openai** - Azure OpenAI Service
- **anthropic** - Anthropic Claude models
- **google-ai** - Google AI (Gemini) models
- **vertex-ai** - Google Vertex AI platform
- **aws-bedrock** - AWS Bedrock service
- **ollama** - Ollama local models
- **openai-generic** - OpenAI-compatible APIs

### Strategy Providers
- **baml-fallback** - Fallback across multiple providers
- **baml-round-robin** - Round-robin load balancing

## Provider Configurations

### OpenAI Provider
```baml
client<llm> GPT4 {
  provider openai
  options {
    model "gpt-4o"
    api_key env.OPENAI_API_KEY
    base_url "https://api.openai.com/v1"  # Optional
    max_tokens 1000                       # Optional
    temperature 0.7                       # Optional
    headers {                            # Optional
      "Custom-Header" "value"
    }
  }
}
```

**Authentication:**
- `api_key env.OPENAI_API_KEY` - Environment variable (recommended)
- `api_key "sk-..."` - Direct API key (not recommended)

**Special Features:**
- O1 models automatically disable streaming
- Supports multimodal inputs (text + images)
- Custom headers support
- Proxy URL support

### Azure OpenAI Provider
```baml
client<llm> AzureGPT {
  provider azure-openai
  options {
    resource_name "your-resource"
    deployment_id "gpt-4-deployment"
    api_version "2024-02-01"
    api_key env.AZURE_OPENAI_API_KEY
    base_url "https://your-resource.openai.azure.com"  # Optional
  }
}
```

**Required Parameters:**
- `resource_name` - Azure resource name
- `deployment_id` - Model deployment ID
- `api_version` - API version
- `api_key` - Azure OpenAI API key

### Anthropic Provider
```baml
client<llm> Claude {
  provider anthropic
  options {
    model "claude-3-5-sonnet-20241022"
    api_key env.ANTHROPIC_API_KEY
    base_url "https://api.anthropic.com"  # Optional
    max_tokens 4096
    headers {                            # Optional
      "anthropic-beta" "prompt-caching-2024-07-31"
    }
  }
}
```

**Features:**
- System message support
- Streaming support
- Custom headers for beta features
- Prompt caching support

### Google AI Provider
```baml
client<llm> Gemini {
  provider google-ai
  options {
    model "gemini-2.5-flash"
    api_key env.GOOGLE_API_KEY
    safetySettings {
      category HARM_CATEGORY_HATE_SPEECH
      threshold BLOCK_LOW_AND_ABOVE
    }
  }
}
```

**Features:**
- Safety settings configuration
- Multimodal support
- Streaming support

### Vertex AI Provider
```baml
client<llm> VertexGemini {
  provider vertex-ai
  options {
    model "gemini-2.5-flash"
    location "us-central1"
    credentials env.GOOGLE_APPLICATION_CREDENTIALS_CONTENT
    project_id "your-project-id"  # Optional
  }
}
```

**Authentication Methods:**
- Service account credentials JSON
- Application default credentials
- Workload identity (in GCP environments)

### AWS Bedrock Provider
```baml
client<llm> BedrockClaude {
  provider aws-bedrock
  options {
    model "anthropic.claude-3-5-sonnet-20240620-v1:0"
    region "us-east-1"                    # Optional
    access_key_id env.AWS_ACCESS_KEY_ID   # Optional
    secret_access_key env.AWS_SECRET_ACCESS_KEY  # Optional
    session_token env.AWS_SESSION_TOKEN   # Optional
    profile "default"                     # Optional
    inference_configuration {
      max_tokens 2048
      temperature 0.7
      top_p 0.9
    }
  }
}
```

**Authentication Methods:**
- IAM credentials (access_key_id + secret_access_key)
- AWS profiles
- Session tokens for temporary credentials
- Instance profiles (when running on EC2)

### Ollama Provider
```baml
client<llm> LocalLlama {
  provider ollama
  options {
    model "llama3.1"
    base_url "http://localhost:11434"  # Optional
  }
}
```

**Features:**
- Local model execution
- No API key required
- Custom base URL support

### Generic OpenAI-Compatible Provider
```baml
client<llm> TogetherAI {
  provider "openai-generic"
  options {
    base_url "https://api.together.ai/v1"
    api_key env.TOGETHER_API_KEY
    model "meta-llama/Llama-3-70b-chat-hf"
  }
}
```

**Compatible Services:**
- Together AI
- OpenRouter
- VLLM
- LM Studio
- Anyscale
- Fireworks AI
- And many more OpenAI-compatible APIs

## Resilience Strategies

### Retry Policies

#### Exponential Backoff
```baml
retry_policy ExponentialRetry {
  max_retries 3
  strategy {
    type exponential_backoff
    delay_ms 1000
    multiplier 2.0
    max_delay_ms 10000
  }
}

client<llm> ResilientGPT {
  provider openai
  retry_policy ExponentialRetry
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY
  }
}
```

#### Constant Delay
```baml
retry_policy ConstantRetry {
  max_retries 5
  strategy {
    type constant_delay
    delay_ms 2000
  }
}
```

### Fallback Strategy
```baml
client<llm> ResilientClient {
  provider baml-fallback
  options {
    strategy [
      GPT4Turbo,    # Try first
      Claude,       # Fallback to second
      Gemini        # Final fallback
    ]
  }
}
```

### Round Robin Strategy
```baml
client<llm> LoadBalanced {
  provider baml-round-robin
  options {
    start 0      # Optional starting index
    strategy [
      GPT4,
      Claude,
      Gemini
    ]
  }
}
```

## Streaming Support

### Streaming Capabilities by Provider
- **OpenAI**: ✅ Full streaming (except O1 models)
- **Anthropic**: ✅ Full streaming
- **Google AI**: ✅ Streaming supported
- **Vertex AI**: ✅ Streaming supported
- **AWS Bedrock**: ✅ Streaming supported
- **Ollama**: ✅ Streaming supported
- **Generic OpenAI**: ✅ Depends on provider
- **Strategy providers**: ❌ No streaming support

### Disable Streaming
```baml
client<llm> NoStreamClient {
  provider openai
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY
    supported_request_modes {
      stream false
    }
  }
}
```

## Model-Specific Features

### OpenAI O1 Models
```baml
client<llm> O1Client {
  provider openai
  options {
    model "o1-preview"
    api_key env.OPENAI_API_KEY
    max_completion_tokens 2000  # Use this instead of max_tokens
  }
}
```

**Limitations:**
- Automatically disable streaming
- Limited to `user` and `assistant` roles
- Use `max_completion_tokens` instead of `max_tokens`

### Anthropic Models
```baml
client<llm> ClaudeWithCaching {
  provider anthropic
  options {
    model "claude-3-5-sonnet-20241022"
    api_key env.ANTHROPIC_API_KEY
    max_tokens 4096  # Required parameter
    headers {
      "anthropic-beta" "prompt-caching-2024-07-31"
    }
  }
}
```

**Features:**
- Support system messages
- Require `max_tokens` parameter
- Support prompt caching with beta headers

### Multimodal Support
- **OpenAI**: Images, audio, video
- **Anthropic**: Images
- **Google AI**: Images, audio, video
- **Vertex AI**: Images, audio, video

## Advanced Configuration

### Custom Headers
```baml
client<llm> CustomClient {
  provider openai
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY
    headers {
      "Custom-Header" "value"
      "Another-Header" env.HEADER_VALUE
    }
  }
}
```

### Proxy Support
```baml
client<llm> ProxyClient {
  provider openai
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY
    proxy_url "https://proxy.example.com"
  }
}
```

### Query Parameters
```baml
client<llm> QueryParamClient {
  provider openai
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY
    query_params {
      "custom_param" "value"
    }
  }
}
```

## Environment Variables

### Common Environment Variables
```bash
# OpenAI
OPENAI_API_KEY=sk-...

# Azure OpenAI
AZURE_OPENAI_API_KEY=...

# Anthropic
ANTHROPIC_API_KEY=sk-ant-...

# Google
GOOGLE_API_KEY=...
GOOGLE_APPLICATION_CREDENTIALS=/path/to/credentials.json

# AWS
AWS_ACCESS_KEY_ID=...
AWS_SECRET_ACCESS_KEY=...
AWS_SESSION_TOKEN=...
AWS_PROFILE=default

# Other Providers
TOGETHER_API_KEY=...
OPENROUTER_API_KEY=...
FIREWORKS_API_KEY=...
```

### Environment Variable Usage
```baml
client<llm> EnvClient {
  provider openai
  options {
    model env.MODEL_NAME        # From environment
    api_key env.OPENAI_API_KEY  # From environment
    base_url env.OPENAI_BASE_URL # Optional from environment
  }
}
```


# Testing Framework

## Test Definition

### Basic Test Structure
```baml
test TestName {
  functions [FunctionName]
  args {
    param1 "test value"
    param2 {
      nested_field "value"
      array_field ["item1", "item2"]
    }
  }
}
```

### Multiple Test Cases
```baml
test ExtractResumeTests {
  functions [ExtractResume]
  args {
    resume_text #"
      John Doe
      Software Engineer
      john@example.com
      Skills: Python, JavaScript
    "#
  }
}

test ExtractResumeEdgeCases {
  functions [ExtractResume]
  args {
    resume_text "Minimal resume with just a name: Jane Smith"
  }
}
```

### Image Input Testing
```baml
test ImageAnalysisTest {
  functions [AnalyzeImage]
  args {
    img {
      url "https://example.com/test-image.png"
    }
  }
}

test LocalImageTest {
  functions [AnalyzeImage]
  args {
    img {
      file "../images/test-image.jpg"
    }
  }
}
```

### Audio Input Testing
```baml
test AudioTranscriptionTest {
  functions [TranscribeAudio]
  args {
    audio {
      url "https://example.com/test-audio.mp3"
    }
  }
}
```

## Running Tests

### Basic Test Execution
```bash
# Run all tests
baml test

# Run tests for specific function
baml test -i "ExtractResume::"

# Run specific test
baml test -i "ExtractResume::ExtractResumeTests"

# Run tests matching pattern
baml test -i "*Resume*"
```

### Test Filtering
```bash
# Include patterns
baml test -i "Extract*"                    # Functions starting with "Extract"
baml test -i "GetWeather::test_sunny_day"  # Specific test in function
baml test -i "::basic_test"                # Test named "basic_test" in any function

# Exclude patterns
baml test -x "*flaky*"                     # Exclude flaky tests
baml test -x "::slow_test"                 # Exclude slow tests

# Combine include and exclude
baml test -i "Extract*" -x "*slow*"
```

### Parallel Testing
```bash
# Run tests in parallel (default: 10)
baml test --parallel 5

# Disable parallel execution
baml test --parallel 1
```

### Environment Configuration
```bash
# Load .env file (default: true)
baml test --dotenv

# Use custom .env file
baml test --dotenv-path ./test.env

# Disable .env loading
baml test --no-dotenv
```

## Test Output and Validation

### Human Evaluation
```bash
# Require human evaluation (default: true)
baml test --require-human-eval

# Allow tests that need human evaluation to pass
baml test --no-require-human-eval
```

### Test Results
```bash
# List tests without running
baml test --list

# Pass if no tests are found
baml test --pass-if-no-tests
```

## Advanced Testing

### Complex Data Structures
```baml
test ComplexDataTest {
  functions [ProcessComplexData]
  args {
    data {
      users [
        {
          name "Alice"
          age 30
          preferences {
            theme "dark"
            notifications true
          }
        },
        {
          name "Bob"
          age 25
          preferences {
            theme "light"
            notifications false
          }
        }
      ]
      metadata {
        version "1.0"
        created_at "2024-01-01T00:00:00Z"
      }
    }
  }
}
```

### Dynamic Test Data
```baml
test DynamicTest {
  functions [ProcessDynamicData]
  args {
    input env.TEST_INPUT  # From environment variable
  }
}
```

### Multi-Function Tests
```baml
test WorkflowTest {
  functions [ExtractData, ProcessData, FormatOutput]
  args {
    raw_text "Sample input text for processing"
  }
}
```

## Test Organization

### File Organization
```
baml_src/
├── functions.baml      # Function definitions
├── types.baml          # Type definitions
├── clients.baml        # Client configurations
└── tests.baml          # Test definitions
```

### Grouping Tests
```baml
// Basic functionality tests
test ExtractResume_Basic {
  functions [ExtractResume]
  args {
    resume_text "Standard resume format..."
  }
}

// Edge case tests
test ExtractResume_EdgeCases {
  functions [ExtractResume]
  args {
    resume_text "Minimal resume..."
  }
}

// Performance tests
test ExtractResume_LargeInput {
  functions [ExtractResume]
  args {
    resume_text "Very long resume with extensive details..."
  }
}
```

## Test Best Practices

### Naming Conventions
```baml
// Good test names
test ExtractResume_StandardFormat
test ExtractResume_MissingEmail
test ExtractResume_MultipleJobs
test AnalyzeImage_Portrait
test AnalyzeImage_Landscape

// Function-specific grouping
test GetWeather_SunnyDay
test GetWeather_RainyDay
test GetWeather_InvalidLocation
```

### Test Data Management
```baml
// Use realistic test data
test ExtractResume_Realistic {
  functions [ExtractResume]
  args {
    resume_text #"
      John Smith
      Senior Software Engineer
      
      Contact Information:
      Email: john.smith@email.com
      Phone: (555) 123-4567
      LinkedIn: linkedin.com/in/johnsmith
      
      Experience:
      - Senior Software Engineer at TechCorp (2020-Present)
        * Led development of microservices architecture
        * Managed team of 5 developers
        * Technologies: Python, Docker, Kubernetes
      
      - Software Engineer at StartupXYZ (2018-2020)
        * Built REST APIs and web applications
        * Technologies: JavaScript, Node.js, React
      
      Skills:
      - Programming: Python, JavaScript, Go
      - Cloud: AWS, Docker, Kubernetes
      - Databases: PostgreSQL, MongoDB
    "#
  }
}
```

### Environment-Specific Tests
```bash
# Development environment
TEST_ENV=dev baml test -i "*dev*"

# Production-like environment
TEST_ENV=prod baml test -i "*prod*"

# Load test-specific environment
baml test --dotenv-path ./test-prod.env
```

## Debugging Tests

### Verbose Output
```bash
# Enable debug logging
BAML_LOG=DEBUG baml test

# JSON formatted logs
BAML_LOG=DEBUG BAML_LOG_JSON_MODE=true baml test
```

### Test Isolation
```bash
# Run single test for debugging
baml test -i "ExtractResume::ExtractResume_Basic"

# Run without parallel execution
baml test --parallel 1 -i "problematic_test"
```

### Error Analysis
```bash
# Continue on test failures to see all results
baml test --continue-on-error

# Save test results to file
baml test > test_results.log 2>&1
```

## Continuous Integration

### CI/CD Integration
```yaml
# GitHub Actions example
name: BAML Tests
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Install BAML
        run: npm install -g @boundaryml/baml
      - name: Run BAML tests
        run: baml test --parallel 5
        env:
          OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
```

### Test Exit Codes
- `0` - All tests passed
- `1` - Human evaluation required
- `2` - Test failures
- `3` - Tests cancelled/interrupted
- `4` - Internal error or invalid arguments
- `5` - No tests found


# Advanced Features

## Multimodal Inputs

### Image Processing
```baml
function AnalyzeImage(img: image, context: string?) -> ImageAnalysis {
  client GPT4o
  prompt #"
    {{ _.role("user") }}
    {% if context %}
    Context: {{ context }}
    {% endif %}
    
    Analyze this image:
    {{ img }}
    
    {{ ctx.output_format }}
  "#
}

class ImageAnalysis {
  description string
  objects_detected string[]
  mood "happy" | "sad" | "neutral" | "excited"
  colors string[]
}
```

### Audio Processing
```baml
function TranscribeAudio(audio: audio, language: string?) -> Transcription {
  client GPT4o
  prompt #"
    {{ _.role("user") }}
    {% if language %}
    Transcribe this audio in {{ language }}:
    {% else %}
    Transcribe this audio:
    {% endif %}
    {{ audio }}
    
    {{ ctx.output_format }}
  "#
}

class Transcription {
  text string
  confidence float
  language string
  duration_seconds float?
}
```

### Video Processing
```baml
function AnalyzeVideo(video: video) -> VideoAnalysis {
  client GPT4o
  prompt #"
    {{ _.role("user") }}
    Analyze this video and provide a summary:
    {{ video }}
    
    {{ ctx.output_format }}
  "#
}

class VideoAnalysis {
  summary string
  key_scenes string[]
  duration_estimate string
  main_subjects string[]
}
```

## Dynamic Types and Schemas

### Dynamic Classes
```baml
class DynamicResponse {
  @@dynamic  // Schema inferred at runtime
}

function FlexibleExtraction(text: string, schema_hint: string) -> DynamicResponse {
  client GPT4o
  prompt #"
    Extract information from the following text based on this schema hint: {{ schema_hint }}
    
    Text: {{ text }}
    
    {{ ctx.output_format }}
  "#
}
```

### Dynamic Enums
```baml
enum DynamicCategory {
  @@dynamic  // Values determined at runtime
}

function CategorizeContent(content: string) -> DynamicCategory {
  client GPT4o
  prompt #"
    Categorize this content into the most appropriate category:
    {{ content }}
    
    {{ ctx.output_format }}
  "#
}
```

### Runtime Type Building
```python
# Python example
from baml_client import type_builder

# Create custom type builder
tb = type_builder.TypeBuilder()

# Use with dynamic function
client = b.with_options(tb=tb)
result = await client.FlexibleExtraction(
    text="...",
    schema_hint="Extract person name, age, and occupation"
)
```

## Constraint System

### Field-Level Constraints
```baml
class ValidatedUser {
  email string @check(valid_email, {{ this | regex_match("^[^@]+@[^@]+\.[^@]+$") }})
  age int @check(adult, {{ this >= 18 }})
        @check(reasonable_age, {{ this <= 120 }})
  username string @check(min_length, {{ this | length >= 3 }})
                 @check(max_length, {{ this | length <= 20 }})
                 @check(alphanumeric, {{ this | regex_match("^[a-zA-Z0-9_]+$") }})
}
```

### Cross-Field Validation
```baml
class DateRange {
  start_date string
  end_date string
  
  @@check(valid_range, {{ this.end_date > this.start_date }})
}

class Event {
  name string
  start_time string
  end_time string
  max_attendees int
  current_attendees int
  
  @@check(time_order, {{ this.end_time > this.start_time }})
  @@check(attendee_limit, {{ this.current_attendees <= this.max_attendees }})
}
```

### Function Parameter Constraints
```baml
function ProcessText(
  text: string @assert(not_empty, {{ this | length > 0 }})
              @assert(max_length, {{ this | length <= 10000 }}),
  language: string @assert(valid_lang, {{ this in ["en", "es", "fr", "de"] }})
) -> ProcessedText {
  client GPT4o
  prompt #"
    Process this {{ language }} text: {{ text }}
    {{ ctx.output_format }}
  "#
}
```

## Advanced Prompt Engineering

### Conditional Prompting
```baml
function AdaptiveResponse(
  user_input: string,
  user_level: "beginner" | "intermediate" | "expert",
  include_examples: bool
) -> string {
  client GPT4o
  prompt #"
    {{ _.role("system") }}
    {% if user_level == "beginner" %}
    You are a patient teacher explaining concepts simply.
    {% elif user_level == "intermediate" %}
    You are a knowledgeable instructor providing detailed explanations.
    {% else %}
    You are an expert consultant providing advanced insights.
    {% endif %}
    
    {{ _.role("user") }}
    {{ user_input }}
    
    {% if include_examples %}
    Please include practical examples in your response.
    {% endif %}
  "#
}
```

### Loop-Based Prompting
```baml
function ProcessConversation(messages: ConversationMessage[]) -> ConversationSummary {
  client GPT4o
  prompt #"
    {{ _.role("system") }}
    Analyze this conversation and provide a summary.
    
    {% for msg in messages %}
    {{ _.role(msg.role) }}
    [{{ msg.timestamp }}] {{ msg.speaker }}: {{ msg.content }}
    {% endfor %}
    
    {{ ctx.output_format }}
  "#
}

class ConversationMessage {
  role "user" | "assistant"
  speaker string
  content string
  timestamp string
}

class ConversationSummary {
  main_topics string[]
  key_decisions string[]
  action_items string[]
  sentiment "positive" | "negative" | "neutral"
}
```

### Template Reuse
```baml
template_string FormatUserProfile(user: User) #"
  Name: {{ user.name }}
  Role: {{ user.role }}
  {% if user.department %}
  Department: {{ user.department }}
  {% endif %}
  Permissions: {{ user.permissions | join(", ") }}
"#

function GenerateUserReport(users: User[]) -> string {
  client GPT4o
  prompt #"
    Generate a user access report for the following users:
    
    {% for user in users %}
    {{ FormatUserProfile(user) }}
    ---
    {% endfor %}
    
    Provide a summary of access levels and any security recommendations.
  "#
}
```

## Error Handling and Resilience

### Comprehensive Error Handling
```python
# Python example
from baml_py import (
    BamlError,
    BamlClientError,
    BamlClientHttpError,
    BamlValidationError,
    BamlClientFinishReasonError
)

async def robust_function_call(input_data):
    max_retries = 3
    retry_count = 0
    
    while retry_count < max_retries:
        try:
            result = await b.ProcessData(input_data)
            return result
            
        except BamlClientHttpError as e:
            if e.status_code >= 500:  # Server error, retry
                retry_count += 1
                await asyncio.sleep(2 ** retry_count)  # Exponential backoff
                continue
            else:  # Client error, don't retry
                raise
                
        except BamlValidationError as e:
            # Try to fix validation issues
            print(f"Validation error: {e}")
            print(f"Raw output: {e.raw_output}")
            # Could implement fallback parsing here
            raise
            
        except BamlClientFinishReasonError as e:
            if e.finish_reason == "content_filter":
                # Handle content filtering
                print("Content was filtered, trying alternative approach")
                # Could modify input and retry
                raise
            else:
                raise
                
        except BamlError as e:
            print(f"General BAML error: {e}")
            raise
    
    raise Exception(f"Failed after {max_retries} retries")
```

### TypeScript Error Handling
```typescript
import { 
  BamlClientHttpError,
  BamlValidationError,
  BamlClientFinishReasonError,
  isBamlError 
} from '@boundaryml/baml'

async function robustFunctionCall(inputData: string) {
  const maxRetries = 3
  let retryCount = 0
  
  while (retryCount < maxRetries) {
    try {
      const result = await b.ProcessData(inputData)
      return result
      
    } catch (error) {
      if (isBamlError(error)) {
        if (error instanceof BamlClientHttpError) {
          if (error.status_code >= 500) {
            retryCount++
            await new Promise(resolve => 
              setTimeout(resolve, Math.pow(2, retryCount) * 1000)
            )
            continue
          }
        } else if (error instanceof BamlValidationError) {
          console.log(`Validation error: ${error.message}`)
          console.log(`Raw output: ${error.raw_output}`)
          // Implement fallback parsing
        }
      }
      throw error
    }
  }
  
  throw new Error(`Failed after ${maxRetries} retries`)
}
```

## Performance Optimization

### Streaming for Large Outputs
```python
# Python streaming example
async def process_large_document(document: str):
    stream = b.stream.AnalyzeLargeDocument(document)
    
    partial_results = []
    async for partial in stream:
        # Process partial results as they arrive
        partial_results.append(partial)
        
        # Could update UI or save intermediate results
        print(f"Progress: {len(partial_results)} chunks processed")
    
    # Get final consolidated result
    final_result = await stream.get_final_response()
    return final_result
```

### Parallel Processing
```python
import asyncio

async def process_multiple_documents(documents: list[str]):
    # Process documents in parallel
    tasks = [
        b.AnalyzeDocument(doc) 
        for doc in documents
    ]
    
    results = await asyncio.gather(*tasks, return_exceptions=True)
    
    # Handle results and exceptions
    successful_results = []
    failed_documents = []
    
    for i, result in enumerate(results):
        if isinstance(result, Exception):
            failed_documents.append((i, documents[i], result))
        else:
            successful_results.append(result)
    
    return successful_results, failed_documents
```

### Caching and Memoization
```python
from functools import lru_cache
import hashlib

class CachedBAMLClient:
    def __init__(self):
        self._cache = {}
    
    async def cached_function_call(self, input_data: str, cache_ttl: int = 3600):
        # Create cache key
        cache_key = hashlib.md5(input_data.encode()).hexdigest()
        
        # Check cache
        if cache_key in self._cache:
            cached_result, timestamp = self._cache[cache_key]
            if time.time() - timestamp < cache_ttl:
                return cached_result
        
        # Call BAML function
        result = await b.ProcessData(input_data)
        
        # Cache result
        self._cache[cache_key] = (result, time.time())
        
        return result
```


# Best Practices and Examples

## Project Structure

### Recommended Directory Layout
```
my-baml-project/
├── baml_src/
│   ├── main.baml           # Main functions
│   ├── types.baml          # Type definitions
│   ├── clients.baml        # LLM client configurations
│   ├── tests.baml          # Test definitions
│   └── generators.baml     # Code generation config
├── baml_client/         # Generated client code
├── .env                 # Environment variables
├── .gitignore
└── README.md
```

## Common Use Cases

### Data Extraction
```baml
function ExtractContactInfo(text: string) -> ContactInfo {
  client GPT4o
  prompt #"
    Extract contact information from: {{ text }}
    {{ ctx.output_format }}
  "#
}

class ContactInfo {
  name string?
  email string? @check(valid_email, {{ this | regex_match("^[^@]+@[^@]+\.[^@]+$") }})
  phone string?
  company string?
}
```

### Content Classification
```baml
function ClassifyContent(content: string) -> ContentClassification {
  client GPT4o
  prompt #"
    Classify: {{ content }}
    {{ ctx.output_format }}
  "#
}

class ContentClassification {
  category "news" | "tech" | "business" | "other"
  confidence float @check(valid_range, {{ this >= 0.0 and this <= 1.0 }})
  tags string[]
}
```

### Conversational AI
```baml
function ChatBot(message: string, history: ChatMessage[]) -> ChatResponse {
  client GPT4o
  prompt #"
    {{ _.role("system") }}
    You are a helpful assistant.
    
    {% for msg in history %}
    {{ _.role(msg.role) }}
    {{ msg.content }}
    {% endfor %}
    
    {{ _.role("user") }}
    {{ message }}
  "#
}

class ChatMessage {
  role "user" | "assistant"
  content string
}

class ChatResponse {
  message string
  suggested_actions string[]?
}
```

## Error Handling Best Practices

### Python Error Handling
```python
from baml_py import BamlError, BamlValidationError, BamlClientHttpError

async def robust_extraction(text: str):
    try:
        return await b.ExtractData(text)
    except BamlValidationError as e:
        # Handle parsing errors
        logger.warning(f"Validation failed: {e.raw_output}")
        return fallback_parse(e.raw_output)
    except BamlClientHttpError as e:
        if e.status_code >= 500:
            # Retry on server errors
            await asyncio.sleep(1)
            return await b.ExtractData(text)
        raise
    except BamlError as e:
        logger.error(f"BAML error: {e}")
        raise
```

### TypeScript Error Handling
```typescript
import { BamlValidationError, BamlClientHttpError, isBamlError } from '@boundaryml/baml'

async function robustExtraction(text: string) {
  try {
    return await b.ExtractData(text)
  } catch (error) {
    if (isBamlError(error)) {
      if (error instanceof BamlValidationError) {
        console.warn(`Validation failed: ${error.raw_output}`)
        return fallbackParse(error.raw_output)
      } else if (error instanceof BamlClientHttpError && error.status_code >= 500) {
        await new Promise(resolve => setTimeout(resolve, 1000))
        return await b.ExtractData(text)
      }
    }
    throw error
  }
}
```

## Performance Optimization

### Batch Processing
```python
async def process_batch(items: list[str], batch_size: int = 5):
    results = []
    for i in range(0, len(items), batch_size):
        batch = items[i:i + batch_size]
        batch_tasks = [b.ProcessItem(item) for item in batch]
        batch_results = await asyncio.gather(*batch_tasks)
        results.extend(batch_results)
        await asyncio.sleep(0.1)  # Rate limiting
    return results
```

### Streaming for Large Content
```python
async def stream_large_analysis(content: str):
    stream = b.stream.AnalyzeLargeContent(content)
    
    async for partial in stream:
        # Process partial results
        yield partial
    
    final = await stream.get_final_response()
    return final
```

## Testing Strategies

### Comprehensive Test Suite
```baml
// Happy path
test ExtractData_Standard {
  functions [ExtractData]
  args { text "John Doe, john@example.com, (555) 123-4567" }
}

// Edge cases
test ExtractData_Minimal {
  functions [ExtractData]
  args { text "John" }
}

test ExtractData_Empty {
  functions [ExtractData]
  args { text "" }
}

// Error conditions
test ExtractData_Garbage {
  functions [ExtractData]
  args { text "asdkjfh random text" }
}
```

### Test Organization
```bash
# Run specific test categories
baml test -i "*_Standard"     # Happy path tests
baml test -i "*_Edge*"        # Edge case tests
baml test -i "*_Error*"       # Error condition tests

# Run tests for specific function
baml test -i "ExtractData::*"

# Exclude slow tests in CI
baml test -x "*_Slow*"
```

## Security Best Practices

### Input Validation
```baml
function SecureAnalysis(
  text: string @assert(safe_length, {{ this | length <= 10000 }})
             @assert(no_scripts, {{ not (this | contains("<script")) }})
) -> Analysis {
  client GPT4o
  prompt #"
    Analyze (sanitized): {{ text | truncate(5000) }}
    {{ ctx.output_format }}
  "#
}
```

### Environment Variables
```bash
# .env
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
BAML_LOG=INFO
BAML_PASSWORD=sk-baml-secure-password
```

### Rate Limiting
```python
from asyncio import Semaphore

class RateLimitedClient:
    def __init__(self, max_concurrent: int = 10):
        self.semaphore = Semaphore(max_concurrent)
    
    async def call(self, func, *args, **kwargs):
        async with self.semaphore:
            return await func(*args, **kwargs)
```

## Monitoring and Observability

### Custom Logging
```python
from baml_py import Collector

class AppCollector(Collector):
    def on_function_log(self, log):
        print(f"Function: {log.function_name}")
        print(f"Duration: {log.timing.duration_ms}ms")
        if log.usage:
            print(f"Tokens: {log.usage.total_tokens}")

collector = AppCollector("my-app")
client = b.with_options(collector=collector)
```

### Metrics Collection
```python
import time
from collections import defaultdict

class Metrics:
    def __init__(self):
        self.calls = defaultdict(list)
    
    async def timed_call(self, name: str, func, *args, **kwargs):
        start = time.time()
        try:
            result = await func(*args, **kwargs)
            success = True
        except Exception as e:
            result = None
            success = False
        finally:
            duration = time.time() - start
            self.calls[name].append({
                'duration': duration,
                'success': success,
                'timestamp': time.time()
            })
        
        if not success:
            raise
        return result
```

# Migration and Upgrade Guide

## Upgrading BAML

### Check Current Version
```bash
baml --version
# or
pip show baml-py
# or
npm list @boundaryml/baml
```

### Upgrade Process
```bash
# Python
pip install --upgrade baml-py

# TypeScript
npm update @boundaryml/baml
# or
yarn upgrade @boundaryml/baml

# Regenerate client code
baml generate
```

### Version Compatibility
- BAML follows semantic versioning
- Generated clients are compatible within major versions
- Breaking changes are documented in CHANGELOG.md

## Troubleshooting

### Common Issues

#### Generation Errors
```bash
# Clear and regenerate
rm -rf baml_client/
baml generate

# Check for syntax errors
baml fmt --dry-run
```

#### Runtime Errors
```bash
# Enable debug logging
BAML_LOG=DEBUG baml test

# Check environment variables
echo $OPENAI_API_KEY
```

#### Performance Issues
```bash
# Run tests with timing
time baml test

# Check parallel execution
baml test --parallel 1  # Disable parallelism
```

### Debug Mode
```python
# Python debug logging
import os
os.environ["BAML_LOG"] = "DEBUG"
os.environ["BAML_LOG_JSON_MODE"] = "true"

# Enable detailed error messages
try:
    result = await b.MyFunction(input="test")
except Exception as e:
    print(f"Error: {e}")
    print(f"Type: {type(e)}")
    if hasattr(e, 'raw_output'):
        print(f"Raw output: {e.raw_output}")
```

## Resources and Community

### Documentation
- [Official Docs](https://docs.boundaryml.com)
- [API Reference](https://docs.boundaryml.com/reference)
- [Examples](https://github.com/BoundaryML/baml/tree/main/integ-tests)

### Community
- [Discord](https://discord.gg/BTNBeXGuaS)
- [GitHub Issues](https://github.com/BoundaryML/baml/issues)
- [GitHub Discussions](https://github.com/BoundaryML/baml/discussions)

### Tools
- [Prompt Fiddle](https://www.promptfiddle.com) - Online BAML playground
- [VS Code Extension](https://marketplace.visualstudio.com/items?itemName=boundaryml.baml)
- [Interactive Examples](https://baml-examples.vercel.app/)

---

**This comprehensive reference covers all aspects of BAML (Boundary AI Markup Language) for building reliable AI workflows and agents. BAML provides type-safe LLM function calls, multi-language support, streaming capabilities, and comprehensive tooling for modern AI application development.**

