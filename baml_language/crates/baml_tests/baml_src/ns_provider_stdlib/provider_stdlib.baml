class ProviderSchemaExample {
  name string
  nickname string?
  tags string[]
}

function provider_schema() -> json {
  baml.json.schema(type.of<ProviderSchemaExample>())
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

test "provider stdlib JSON paths preserve brackets in quoted keys" {
  let value: json = {
    "key[part]": {
      "tail]": 7,
    },
  }
  assert.equal(
    baml.json.path<int>(value, ".[\"key[part]\"][\"tail]\"]"),
    7,
  )
}

test "provider stdlib exposes schema aligned parsing" {
  assert.equal(baml.sap.parse<int>("42"), 42)
}
