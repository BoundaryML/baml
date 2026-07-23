class SchemaJsonExample {
  name: string,
  nickname: string?,
  tags: string[],
}

class SchemaRecursiveEmployee {
  name: string,
  manager: SchemaRecursiveEmployee?,
}

class SchemaMutualA {
  label: string,
  b: SchemaMutualB?,
}

class SchemaMutualB {
  count: int,
  a: SchemaMutualA?,
}

enum SchemaRisk {
  Low
  Medium
  High
}

class SchemaEnumContainer {
  name: string,
  risk: SchemaRisk,
  children: SchemaEnumContainer[],
}

function schema_json_standard() -> json throws baml.errors.Unsupported {
  baml.schema.json_schema(reflect.type_of<SchemaJsonExample>())
}

function schema_json_customized() -> json throws baml.errors.Unsupported {
  let schema = schema_json_standard();
  match (schema) {
    let object: map<string, json> => {
      object.set("x-provider-dialect", "weird-v1");
      object
    },
    _ => schema,
  }
}

function schema_json_recursive() -> json throws baml.errors.Unsupported {
  baml.schema.json_schema(reflect.type_of<SchemaRecursiveEmployee>())
}

function schema_json_mutually_recursive() -> json throws baml.errors.Unsupported {
  baml.schema.json_schema(reflect.type_of<SchemaMutualA>())
}

function schema_json_openai_map_error() -> string throws never {
  root.ai.openai_strict_schema(reflect.type_of<map<string, int>>()) catch (e) {
    let unsupported: baml.errors.Unsupported => return unsupported.message,
  };
  "missing error"
}

test "json_schema returns standard mutable JSON" {
  let schema = schema_json_standard();
  assert.equal(baml.json.path<string>(schema, ".type"), "object");
  assert.equal(baml.json.path<string[]>(schema, ".required"), ["name", "tags"]);
  assert.equal(baml.json.path_or<bool>(schema, ".additionalProperties", true), true);
  assert.equal(
    baml.json.path<string>(schema_json_customized(), ".[\"x-provider-dialect\"]"),
    "weird-v1",
  )
}

test "json_schema preserves recursive class constraints with refs" {
  let schema = schema_json_recursive();
  assert.equal(
    baml.json.path<string>(schema, ".properties.manager.anyOf[0].[\"$ref\"]"),
    "#/$defs/schema_json.SchemaRecursiveEmployee",
  );
  assert.equal(
    baml.json.path<string>(
      schema,
      ".[\"$defs\"].[\"schema_json.SchemaRecursiveEmployee\"].properties.name.type",
    ),
    "string",
  );
  assert.equal(
    baml.json.path<string>(
      schema,
      ".[\"$defs\"].[\"schema_json.SchemaRecursiveEmployee\"].properties.manager.anyOf[0].[\"$ref\"]",
    ),
    "#/$defs/schema_json.SchemaRecursiveEmployee",
  )
}

test "json_schema preserves mutually recursive class constraints" {
  let schema = schema_json_mutually_recursive();
  assert.equal(
    baml.json.path<string>(schema, ".properties.b.anyOf[0].[\"$ref\"]"),
    "#/$defs/schema_json.SchemaMutualB",
  );
  assert.equal(
    baml.json.path<string>(
      schema,
      ".[\"$defs\"].[\"schema_json.SchemaMutualB\"].properties.a.anyOf[0].[\"$ref\"]",
    ),
    "#/$defs/schema_json.SchemaMutualA",
  );
  assert.equal(
    baml.json.path<string>(
      schema, ".[\"$defs\"].[\"schema_json.SchemaMutualA\"].properties.label.type",
    ),
    "string",
  )
}

test "OpenAI transformation closes objects and requires nullable fields" {
  let schema = root.ai.openai_strict_schema(reflect.type_of<SchemaJsonExample>());
  assert.equal(baml.json.path<bool>(schema, ".additionalProperties"), false);
  assert.equal(baml.json.path<string[]>(schema, ".required"), ["name", "nickname", "tags"]);
  assert.equal(baml.json.path<string[]>(schema, ".properties.nickname.type"), ["string", "null"])
}

test "OpenAI transformation preserves and closes recursive definitions" {
  let schema = root.ai.openai_strict_schema(reflect.type_of<SchemaRecursiveEmployee>());
  assert.equal(baml.json.path<bool>(schema, ".additionalProperties"), false);
  assert.equal(
    baml.json.path<bool>(
      schema,
      ".[\"$defs\"].[\"schema_json.SchemaRecursiveEmployee\"].additionalProperties",
    ),
    false,
  );
  assert.equal(
    baml.json.path<string>(schema, ".properties.manager.anyOf[0].[\"$ref\"]"),
    "#/$defs/schema_json.SchemaRecursiveEmployee",
  )
}

test "OpenAI Chat response_format wraps the transformed schema" {
  let format = root.ai.openai_chat_response_format(
    reflect.type_of<SchemaJsonExample>(), "SchemaJsonExample",
  );
  assert.equal(baml.json.path<string>(format, ".type"), "json_schema");
  assert.equal(baml.json.path<string>(format, ".json_schema.name"), "SchemaJsonExample");
  assert.equal(baml.json.path<bool>(format, ".json_schema.strict"), true);
  assert.equal(baml.json.path<bool>(format, ".json_schema.schema.additionalProperties"), false)
}

test "OpenAI strict output tool wraps top-level unions under value" {
  let tool = root.ai.openai_strict_output_tool(
    reflect.type_of<SchemaMutualA | SchemaMutualB>(), "__baml_return_output",
  );
  assert.equal(baml.json.path<string>(tool, ".type"), "function");
  assert.equal(baml.json.path<bool>(tool, ".function.strict"), true);
  assert.equal(
    baml.json.path<string>(tool, ".function.parameters.type"), "object",
  );
  assert.equal(
    baml.json.path<string>(
      tool, ".function.parameters.properties.value.anyOf[0].[\"$ref\"]",
    ),
    "#/$defs/schema_json.SchemaMutualA",
  );
  assert.equal(
    baml.json.path<bool>(
      tool,
      ".function.parameters.[\"$defs\"].[\"schema_json.SchemaMutualA\"].additionalProperties",
    ),
    false,
  )
}

test "OpenAI strict output tool lifts recursive definitions beside value" {
  let tool = root.ai.openai_strict_output_tool(
    reflect.type_of<SchemaRecursiveEmployee>(), "__baml_return_output",
  );
  assert.equal(
    baml.json.path<string>(
      tool,
      ".function.parameters.properties.value.properties.manager.anyOf[0].[\"$ref\"]",
    ),
    "#/$defs/schema_json.SchemaRecursiveEmployee",
  );
  assert.equal(
    baml.json.path<string>(
      tool,
      ".function.parameters.[\"$defs\"].[\"schema_json.SchemaRecursiveEmployee\"].properties.name.type",
    ),
    "string",
  )
}

test "OpenAI strict output tool wraps primitive root types" {
  let boolean_tool = root.ai.openai_strict_output_tool(
    reflect.type_of<bool>(), "__baml_return_bool",
  );
  let string_tool = root.ai.openai_strict_output_tool(
    reflect.type_of<string>(), "__baml_return_string",
  );
  assert.equal(
    baml.json.path<string>(boolean_tool, ".function.parameters.properties.value.type"),
    "boolean",
  );
  assert.equal(
    baml.json.path<string>(string_tool, ".function.parameters.properties.value.type"),
    "string",
  );
  assert.equal(
    baml.json.path<string[]>(boolean_tool, ".function.parameters.required"),
    ["value"],
  )
}

test "OpenAI strict output tool preserves a root enum" {
  let tool = root.ai.openai_strict_output_tool(
    reflect.type_of<SchemaRisk>(), "__baml_return_enum",
  );
  assert.equal(
    baml.json.path<string>(tool, ".function.parameters.properties.value.type"),
    "string",
  );
  assert.equal(
    baml.json.path<string[]>(tool, ".function.parameters.properties.value.enum"),
    ["Low", "Medium", "High"],
  )
}

test "OpenAI strict output tool preserves enum fields in recursive classes" {
  let tool = root.ai.openai_strict_output_tool(
    reflect.type_of<SchemaEnumContainer>(), "__baml_return_recursive_enum",
  );
  assert.equal(
    baml.json.path<string[]>(tool, ".function.parameters.properties.value.properties.risk.enum"),
    ["Low", "Medium", "High"],
  );
  assert.equal(
    baml.json.path<string>(
      tool,
      ".function.parameters.properties.value.properties.children.items.[\"$ref\"]",
    ),
    "#/$defs/schema_json.SchemaEnumContainer",
  );
  assert.equal(
    baml.json.path<string[]>(
      tool,
      ".function.parameters.[\"$defs\"].[\"schema_json.SchemaEnumContainer\"].required",
    ),
    ["name", "risk", "children"],
  )
}

test "OpenAI strict output tool keeps a list of unions in one value array" {
  let tool = root.ai.openai_strict_output_tool(
    reflect.type_of<(SchemaMutualA | SchemaMutualB)[]>(), "__baml_return_union_list",
  );
  assert.equal(
    baml.json.path<string>(tool, ".function.parameters.properties.value.type"),
    "array",
  );
  assert.equal(
    baml.json.path<string>(
      tool,
      ".function.parameters.properties.value.items.anyOf[0].[\"$ref\"]",
    ),
    "#/$defs/schema_json.SchemaMutualA",
  );
  assert.equal(
    baml.json.path<string>(
      tool,
      ".function.parameters.properties.value.items.anyOf[1].[\"$ref\"]",
    ),
    "#/$defs/schema_json.SchemaMutualB",
  )
}

test "OpenAI transformation rejects standard map schemas instead of weakening them" {
  assert.is_true(schema_json_openai_map_error().includes("map-like object"))
}
