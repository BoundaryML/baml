```
client <llm> Foo {
  provider "openai"
  options {
    base_url "openai.com"
    http {
      connect_timeout_ms 1000000
    }
  }
}
```
