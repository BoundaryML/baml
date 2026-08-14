function Foo(bla: int) -> int {
    client GPT4
    prompt `Return 4`
}


client GPT4 = openai.ResponsesClient.new(
    model = "gpt-4o",
    api_key = env.OPENAI_API_KEY,
);

test FooTest {
  functions [Foo]
  args {}
}

//----
//- diagnostics
// <no-diagnostics-expected>
