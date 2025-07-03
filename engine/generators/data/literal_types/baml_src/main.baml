// Test literal types in BAML

class StringLiterals {
  status "active" | "inactive" | "pending"
  environment "dev" | "staging" | "prod"
  method "GET" | "POST" | "PUT" | "DELETE"
}

class IntegerLiterals {
  priority 1 | 2 | 3 | 4 | 5
  httpStatus 200 | 201 | 400 | 404 | 500
  maxRetries 0 | 1 | 3 | 5
}

class BooleanLiterals {
  alwaysTrue true
  alwaysFalse false
  eitherBool true | false
}

class MixedLiterals {
  id int
  type "user" | "admin" | "guest"
  level 1 | 2 | 3
  isActive true | false
  apiVersion "v1" | "v2" | "v3"
}

class ComplexLiterals {
  state "draft" | "published" | "archived" | "deleted"
  retryCount 0 | 1 | 2 | 3 | 5 | 8 | 13  // Fibonacci sequence
  response "success" | "error" | "timeout"
  flags (true | false)[]
  codes (200 | 404 | 500)[]
}

function TestStringLiterals(input: string) -> StringLiterals {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a StringLiterals object with:
    - status: "active"
    - environment: "prod"
    - method: "POST"
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestIntegerLiterals(input: string) -> IntegerLiterals {
  client "openai/gpt-4o-mini"
  prompt #"
    Return an IntegerLiterals object with:
    - priority: 3
    - httpStatus: 201
    - maxRetries: 3
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestBooleanLiterals(input: string) -> BooleanLiterals {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a BooleanLiterals object with:
    - alwaysTrue: true
    - alwaysFalse: false
    - eitherBool: true
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestMixedLiterals(input: string) -> MixedLiterals {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a MixedLiterals object with:
    - id: 12345
    - type: "admin"
    - level: 2
    - isActive: true
    - apiVersion: "v2"
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestComplexLiterals(input: string) -> ComplexLiterals {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a ComplexLiterals object with:
    - state: "published"
    - retryCount: 5
    - response: "success"
    - flags: [true, false, true]
    - codes: [200, 404, 200]
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}