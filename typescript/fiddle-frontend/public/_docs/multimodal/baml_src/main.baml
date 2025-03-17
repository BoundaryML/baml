function DescribeMedia(img: image) -> string {
  client "openai/gpt-4o"
  // Most LLM providers require images or audio to be sent as "user" messages.
  prompt #"
    {{_.role("user")}}
    Describe this image: {{ img }}
  "#
}

// See the "testing functions" Guide for more on testing Multimodal functions
test MyTest {
  functions [DescribeMedia]
  args {
    img {
      url "https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png"
    }
  }
}
