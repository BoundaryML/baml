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

class ProviderSchemaAddress {
  city string
}

class ProviderSchemaVenue {
  name string
  address ProviderSchemaAddress?
}

/// Books a venue.
function provider_schema_book(
  venue: ProviderSchemaVenue,
  backup: ProviderSchemaVenue?,
  billing: ProviderSchemaAddress,
) -> string {
  venue.name
}

// Each parameter is lowered by `baml.json.schema`, whose `$ref`s point at the
// schema root. The definitions they name must be at the tool schema's root,
// scoped by parameter, and not under the parameter that reached them.
function provider_schema_tool_definitions() -> string {
  let schema = baml.json.to_json(ai.tools.tool(provider_schema_book).input_schema)
  let reference = baml.json.path<string>(schema, ".properties.venue.properties.address.anyOf[0].$ref")
  let name = reference.replace("#/$defs/", "")
  let root_definitions = baml.json.path_or<map<string, unknown>>(schema, ".$defs", {})
  let resolved = baml.json.to_json(root_definitions.get(name))
  let nested = baml.json.path_or<map<string, unknown>>(schema, ".properties.venue.$defs", {}).length()
    + baml.json.path_or<map<string, unknown>>(schema, ".properties.backup.$defs", {}).length()
    + baml.json.path_or<map<string, unknown>>(schema, ".properties.billing.$defs", {}).length()
  `${reference.starts_with("#/$defs/")}|${root_definitions.length()}|${baml.json.path_or<string>(resolved, ".properties.city.type", "unresolved")}|${nested}`
}

test "a reflected tool schema keeps its definitions at the root" {
  assert.equal(provider_schema_tool_definitions(), "true|3|string|0")
}

/// Accepts two independently reflected parameter types.
function provider_schema_pair<A, B>(first: A, second: B) -> string {
  "ok"
}

test "a reflected tool keeps same-name parameter definitions distinct" {
  let left = reflect.class.new("Collision", { "left": reflect.Type.of<int>() });
  let right = reflect.class.new("Collision", { "right": reflect.Type.of<string>() });
  type Left = unreflect(left.as_type())
  type Right = unreflect(right.as_type())
  let schema = baml.json.to_json(ai.tools.tool(provider_schema_pair<Left[], Right[]>).input_schema);
  let left_ref = baml.json.path<string>(schema, ".properties.first.items.$ref");
  let right_ref = baml.json.path<string>(schema, ".properties.second.items.$ref");
  let definitions = baml.json.path<map<string, json>>(schema, ".$defs");
  assert.is_true(left_ref != right_ref);
  assert.equal(baml.json.path<string>(definitions.get(left_ref.replace("#/$defs/", "")), ".properties.left.type"), "integer");
  assert.equal(baml.json.path<string>(definitions.get(right_ref.replace("#/$defs/", "")), ".properties.right.type"), "string");
}

class ProviderSchemaNode {
  value string @description("Keep #/$defs/Node as documentation.")
  next ProviderSchemaNode?
}

test "a reflected tool rebases recursive refs without changing documentation" {
  let schema = baml.json.to_json(ai.tools.tool(provider_schema_pair<ProviderSchemaNode, ProviderSchemaNode>).input_schema);
  let definitions = baml.json.path<map<string, json>>(schema, ".$defs");
  let left_ref = baml.json.path<string>(schema, ".properties.first.properties.next.anyOf[0].$ref");
  let right_ref = baml.json.path<string>(schema, ".properties.second.properties.next.anyOf[0].$ref");
  assert.is_true(left_ref != right_ref);
  for (let reference in [left_ref, right_ref]) {
    let definition = definitions.get(reference.replace("#/$defs/", ""));
    assert.equal(baml.json.path<string>(definition, ".properties.next.anyOf[0].$ref"), reference);
    assert.equal(baml.json.path<string>(definition, ".properties.value.description"), "Keep #/$defs/Node as documentation.");
  }
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
