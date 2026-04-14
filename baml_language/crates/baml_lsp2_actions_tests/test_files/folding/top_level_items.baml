class Address {
  street string
  city string
  zip string
}

class Person {
  name string
  address Address
}

enum Status {
  Active
  Inactive
  Pending
}

client<llm> MyClient {
  provider openai
  options {
    model "gpt-4"
    temperature 0.7
  }
}

template_string Greeting(name: string) #"
  Hello, {{ name }}!
  Welcome to our service.
"#

retry_policy ExponentialBackoff {
  max_retries 3
  strategy {
    type exponential_backoff
    delay_ms 100
    max_delay_ms 5000
  }
}

function greet(name: string) -> string {
  "Hello, " + name
}

test TestGreet {
  functions [greet]
  args {
    name "World"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- folding_ranges
// top_level_items.baml:1:1-5:2
// top_level_items.baml:7:1-10:2
// top_level_items.baml:12:1-16:2
// top_level_items.baml:18:1-24:2
// top_level_items.baml:26:1-29:3
// top_level_items.baml:26:25-26:39
// top_level_items.baml:31:1-38:2
// top_level_items.baml:40:1-42:2
// top_level_items.baml:40:15-40:29
// top_level_items.baml:40:40-42:2
// top_level_items.baml:44:1-49:2
