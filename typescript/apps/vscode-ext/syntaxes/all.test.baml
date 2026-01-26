// here is a comment

class OtherClass {
  inner string
}

class MyClass {
  @@alias("this is an alias")

  field int
  field null
  field string
  field bool
  field float
  field OtherClass

  field OtherClass?

  field string[]

  field (int | string | OtherClass)
  field (int | string | OtherClass)[]
  field (int | string | OtherClass)?

  field map<string, string>
  field map<string, OtherClass>
  field map<string, map<string, string>>
}

function CallGpt(input: string) -> string[] {
  client GptProvider
  prompt #"
    Do something with {{ input }}

    {{ ctx.output_format }}
  "#
}

function CallClaude(input: string) -> string[] {
  client "anthropic/claude-3-5"
  prompt #"
    Do something with {{ input }}

    {{ ctx.output_format }}
  "#
}

test CallGptTest {
  functions [CallGpt]
  args {
    input "hello"
  }
}

// Test new language features

// Expression function
function ModernFunction(x: int) -> int {
  # Main Processing Section

  ## Loop Processing
  // For-in loop
  for (item in [1, 2, 3]) {
    item += 1;
  }

  ### Advanced Loop Example
  // C-style for loop
  for (let i = 0; i < 10; i++) {
    i += 2;
  }

  ## Object Operations
  // Object construction with fields
  let receipt = Receipt {
    establishment_name: "Pizzaria la Rocca",
    date: "2025-09-03",
    total: 25.50,
    currency: "USD",
  };

  ### Method Invocation
  // Method call
  let result = receipt.calculate();

  ## Mathematical Operations
  // Assignment operators
  result += 5;
  result -= 2;
  result *= 3;
  result /= 4;

  ## Final Result Processing
  // Binary operators
  if (result > 10 && result < 100) {
    result
  } else {
    0
  }
}

// Traditional function keyword still works
function TraditionalFunction() -> string {
  let obj = MyObject { id: 42, name: "test" };
  let value = obj.getValue();
  value
}
