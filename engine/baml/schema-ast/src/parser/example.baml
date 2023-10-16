// This is preferred
client<llm> SomeOtherClient {
  provider openai
  retry    4
  options {
		model gpt-3-test
    my_custom_var some string with spaces
    dictionary_key {
			array_key ["abc", "abc", aaa]
			key2 some random value
      "key with space" is this value
    }
  }
} @default
