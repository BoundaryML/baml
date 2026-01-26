// Test mixed complex type combinations in BAML

class KitchenSink {
  // Primitives
  id int
  name string
  score float
  active bool
  nothing null

  // Literals
  status "draft" | "published" | "archived"
  priority 1 | 2 | 3 | 4 | 5

  // Arrays
  tags string[]
  numbers int[]
  matrix int[][]

  // Maps
  metadata map<string, string>
  scores map<string, float>

  // Optional/Nullable
  description string?
  notes string | null

  // Unions
  data string | int | DataObject
  result Success | Error

  // Complex nested
  user User
  items Item[]
  config Configuration
}

class DataObject {
  type "object"
  value map<string, string>
}

class Success {
  type "success"
  data map<string, string>
}

class Error {
  type "error"
  message string
  code int
}

class User {
  id int
  profile UserProfile
  settings map<string, Setting>
}

class UserProfile {
  name string
  email string
  // TODO: Go generator doesn't support image type yet
  // avatar image?
  bio string?
  links string[]
}

class Setting {
  key string
  value string | int | bool
  metadata map<string, string>?
}

class Item {
  id int
  name string
  variants Variant[]
  attributes map<string, string | int | float | bool>
}

class Variant {
  sku string
  price float
  stock int
  options map<string, string>
}

class Configuration {
  version string
  features Feature[]
  environments map<string, Environment>
  rules Rule[]
}

class Feature {
  name string
  enabled bool
  config map<string, string | int | bool>?
  dependencies string[]
}

class Environment {
  name string
  url string
  variables map<string, string>
  secrets map<string, string>?
}

class Rule {
  id int
  name string
  condition Condition
  actions Action[]
  priority int
}

class Condition {
  type "and" | "or" | "not"
  conditions (Condition | SimpleCondition)[]
}

class SimpleCondition {
  field string
  operator "eq" | "ne" | "gt" | "lt" | "contains"
  value string | int | float | bool
}

class Action {
  type string
  parameters map<string, string | int | bool>
  async_ bool @alias("async")
}

class UltraComplex {
  // Recursive union with arrays and maps
  tree Node

  // Discriminated union with nested complexity
  widgets Widget[]

  // Multi-level optional/nullable
  data ComplexData?

  // Response structure
  response UserResponse

  // Mixed media
  assets Asset[]
}

class Node {
  id int
  type "leaf" | "branch"
  value string | int | Node[] | map<string, Node>
  metadata NodeMetadata?
}

class NodeMetadata {
  created string
  modified string
  tags string[]
  attributes map<string, string | int | bool | null>
}

class Widget {
  type "button" | "text" | "image" | "container"
  button ButtonWidget?
  text TextWidget?
  img ImageWidget?
  container ContainerWidget?
}

class ButtonWidget {
  label string
  action string
  style map<string, string>
}

class TextWidget {
  content string
  format "plain" | "markdown" | "html"
  style map<string, string>
}

class ImageWidget {
  // TODO: Go generator doesn't support image type yet
  // source image
  alt string
  dimensions Dimensions
}

class Dimensions {
  width int
  height int
}

class ContainerWidget {
  layout "flex" | "grid" | "stack"
  children Widget[]
  style map<string, string>
}

class ComplexData {
  primary PrimaryData
  secondary SecondaryData?
  tertiary TertiaryData | null
}

class PrimaryData {
  values (string | int | float)[]
  mappings map<string, map<string, string>>
  flags bool[]
}

class SecondaryData {
  records Record[]
  index map<string, Record>
}

class Record {
  id int
  data map<string, string | int | bool | null>
  related Record[]?
}

class TertiaryData {
  raw string
  parsed map<string, string>?
  valid bool
}

// User response
class UserResponse {
  status "success" | "error"
  data User?
  error ErrorDetail?
  metadata ResponseMetadata
}

class ErrorDetail {
  code string
  message string
  details map<string, string>?
}

class ResponseMetadata {
  timestamp string
  requestId string
  duration int
  retries int
}

class Asset {
  id int
  type "image" | "audio" | "document"
  // TODO: Go generator doesn't support image/audio types yet
  // media image | audio | null
  metadata AssetMetadata
  tags string[]
}

class AssetMetadata {
  filename string
  size int
  mimeType string
  uploaded string
  checksum string
}

function TestKitchenSink(input: string) -> KitchenSink {
  client "openai/gpt-4o-mini"
  prompt #"
    Create a KitchenSink object with all fields populated with realistic test data.
    Mix different types appropriately in unions and complex fields.

    {{ ctx.output_format }}

    Input: {{ input }}
  "#
}

function TestUltraComplex(input: string) -> UltraComplex {
  client "openai/gpt-4o-mini"
  prompt #"
    Create an UltraComplex object demonstrating:
    - A tree structure with mixed node types
    - Various widget types in the array
    - Complex nested data structures
    - A successful response with user data
    - Mixed media assets

    {{ ctx.output_format }}

    Input: {{ input }}
  "#
}

function TestRecursiveComplexity(input: string) -> Node {
  client "openai/gpt-4o-mini"
  prompt #"
    Create a Node tree structure that demonstrates:
    - Leaf nodes with string and int values
    - Branch nodes with array children
    - Nodes with map values containing other nodes
    - At least 3 levels of nesting

    {{ ctx.output_format }}

    Input: {{ input }}
  "#
}
