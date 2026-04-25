// 30+ class trees with cross-class refs, self-references, optional /
// list / map / union nesting. Class-only: the original fixture's
// non-LLM functions (TestKitchenSink / TestUltraComplex /
// TestRecursiveComplexity) are unrepresentable in user BAML — `void`
// returns and bodyless declarations don't exist in the grammar — so
// they're dropped here. Function emission coverage lives in
// `python_example_09a` / `python_llm_functions`.

class KitchenSink {
  id int
  name string
  score float
  active bool
  status string
  priority int
  tags string[]
  numbers int[]
  matrix int[][]
  metadata map<string, string>
  scores map<string, float>
  description string?
  data string | int | DataObject
  result Success | Error
  user User
  items Item[]
  config Configuration
}

class DataObject {
  type_ string
  value map<string, string>
}

class Success {
  type_ string
  data map<string, string>
}

class Error {
  type_ string
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
  bio string?
  links string[]
}

class Setting {
  key string
  value string | int | bool
  metadata (map<string, string>)?
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
  config (map<string, string | int | bool>)?
  dependencies string[]
}

class Environment {
  name string
  url string
  variables map<string, string>
  secrets (map<string, string>)?
}

class Rule {
  id int
  name string
  condition Condition
  actions Action[]
  priority int
}

class Condition {
  type_ string
  conditions (Condition | SimpleCondition)[]
}

class SimpleCondition {
  field string
  operator string
  value string | int | float | bool
}

class Action {
  type_ string
  parameters map<string, string | int | bool>
  async_ bool
}

class UltraComplex {
  tree Node
  widgets Widget[]
  data ComplexData?
  response UserResponse
  assets Asset[]
}

class Node {
  id int
  type_ string
  value string | int | Node[] | map<string, Node>
  metadata NodeMetadata?
}

class NodeMetadata {
  created string
  modified string
  tags string[]
  attributes map<string, string | int | bool>
}

class Widget {
  type_ string
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
  format string
  style map<string, string>
}

class ImageWidget {
  alt string
  dimensions Dimensions
}

class Dimensions {
  width int
  height int
}

class ContainerWidget {
  layout string
  children Widget[]
  style map<string, string>
}

class ComplexData {
  primary PrimaryData
  secondary SecondaryData?
  tertiary TertiaryData?
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
  data map<string, string | int | bool>
  related (Record[])?
}

class TertiaryData {
  raw string
  parsed (map<string, string>)?
  valid bool
}

class UserResponse {
  status string
  data User?
  error ErrorDetail?
  metadata ResponseMetadata
}

class ErrorDetail {
  code string
  message string
  details (map<string, string>)?
}

class ResponseMetadata {
  timestamp string
  requestId string
  duration int
  retries int
}

class Asset {
  id int
  type_ string
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
