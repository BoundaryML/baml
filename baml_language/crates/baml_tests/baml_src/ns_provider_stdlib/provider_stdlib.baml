class ProviderSchemaExample {
  name string
  nickname string?
  tags string[]
}

function provider_schema() -> json {
  baml.json.schema(reflect.Type.of<ProviderSchemaExample>())
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

/// The kind of place the user trains in.
enum ProviderSchemaGymType {
  COMMERCIAL @description("Commercial gym")
  GARAGE @description("Garage gym")
  CUSTOM
}

class ProviderSchemaGym {
  @@description("A gym to save for the user.")
  name string @description("Short gym name.")
  gym_type ProviderSchemaGymType @description("Where the user trains.")
  notes string?
}

/// Saves a gym.
function provider_schema_save_gym(gym: ProviderSchemaGym) -> string {
  gym.name
}

// A tool's schema is reflected from its signature, so the documentation on the
// argument types is what tells the model how to fill them in.
function provider_schema_tool_descriptions() -> string[] {
  let schema = baml.json.to_json(ai.tools.tool(provider_schema_save_gym).input_schema)
  let gym = baml.json.path<string>(schema, ".properties.gym.description")
  let name = baml.json.path<string>(schema, ".properties.gym.properties.name.description")
  let gym_type = baml.json.path<string>(schema, ".properties.gym.properties.gym_type.description")
  let notes = baml.json.path_or<string>(schema, ".properties.gym.properties.notes.description", "none");
  [gym, name, gym_type, notes]
}

test "a reflected tool schema carries class, field and enum documentation" {
  assert.equal(
    provider_schema_tool_descriptions(),
    [
      "A gym to save for the user.",
      "Short gym name.",
      "Where the user trains.\n\nThe kind of place the user trains in.\n\n- COMMERCIAL: Commercial gym\n- GARAGE: Garage gym",
      "none",
    ],
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
