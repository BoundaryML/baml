class ProviderSchemaExample {
  name string
  nickname string?
  tags string[]
}

function provider_schema() -> json {
  baml.schema.json_schema(reflect.type_of<ProviderSchemaExample>())
}

function provider_schema_values() -> string {
  let schema = provider_schema()
  baml.json.path<string>(schema, ".type")
    + "|"
    + baml.json.path<string>(schema, ".properties.name.type")
    + "|"
    + baml.json.path<string[]>(schema, ".required").join(",")
    + "|"
    + baml.json.path_or<string>(
      schema,
      ".properties.missing.type",
      "missing",
    )
}

test "provider stdlib exposes typed JSON paths and schemas" {
  assert.equal(
    provider_schema_values(),
    "object|string|name,tags|missing",
  )
}

test "provider stdlib exposes schema aligned parsing" {
  assert.equal(baml.sap.parse<int>("42"), 42)
}

function provider_prompt() -> baml.llm.PromptAst {
  let context_client = baml.llm.ContextClient {
    name: "test",
    provider: "test",
    default_role: "system",
    allowed_roles: ["system", "user"],
  }
  let context = baml.llm.Context { client: context_client, tags: {} }
  let render = prompt`${role("system")}Be concise.${role("user")}Hello!`
  render(context)
}

test "provider stdlib flattens rendered prompts" {
  assert.equal(
    baml.llm.prompt_to_text(provider_prompt()),
    "Be concise.\nHello!",
  )
}
