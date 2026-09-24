client MyClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function main() -> string {
    MyClient.id()
}

test "client_declaration_methods_dispatch" {
    assert.equal(main(), "openai/gpt-4o-mini")
}
