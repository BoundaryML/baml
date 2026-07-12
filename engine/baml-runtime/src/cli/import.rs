use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::{Command, Output},
};

use anyhow::{bail, Context, Result};
use internal_baml_core::baml_keywords;
use serde_json::{Map, Value};

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum ImportSource {
    Jsonschema,
    Pydantic,
}

#[derive(clap::Args, Debug)]
pub struct ImportArgs {
    #[arg(long = "from", value_enum, help = "Schema source format")]
    pub source: ImportSource,

    #[arg(help = "Path to the schema or model file")]
    pub input: PathBuf,

    #[arg(
        long,
        default_value = "baml_src/generated.baml",
        help = "Path for the generated BAML file"
    )]
    pub out: PathBuf,
}

impl ImportArgs {
    pub fn run(&self) -> Result<()> {
        let schema = match self.source {
            ImportSource::Jsonschema => read_json_schema(&self.input)?,
            ImportSource::Pydantic => pydantic_json_schema(&self.input)?,
        };
        let fallback_name = self
            .input
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("ImportedSchema");
        let baml = json_schema_to_baml(&schema, fallback_name)?;

        if let Some(parent) = self
            .out
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create output directory {}", parent.display())
            })?;
        }
        std::fs::write(&self.out, baml)
            .with_context(|| format!("Failed to write {}", self.out.display()))?;
        println!("Generated {}", self.out.display());
        Ok(())
    }
}

fn read_json_schema(path: &Path) -> Result<Value> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read JSON Schema from {}", path.display()))?;
    serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse JSON Schema from {}", path.display()))
}

const PYDANTIC_SCHEMA_SCRIPT: &str = r##"
import importlib.util
import inspect
import json
import pathlib
import sys

from pydantic import BaseModel

path = pathlib.Path(sys.argv[1]).resolve()
sys.path.insert(0, str(path.parent))
spec = importlib.util.spec_from_file_location("_baml_import_models", path)
if spec is None or spec.loader is None:
    raise RuntimeError(f"Could not load {path}")
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)

models = []
for value in vars(module).values():
    if inspect.isclass(value) and value is not BaseModel and issubclass(value, BaseModel):
        if value.__module__ == module.__name__:
            models.append(value)

if not models:
    raise RuntimeError(f"No Pydantic BaseModel subclasses found in {path}")

definitions = {}
for model in models:
    if hasattr(model, "model_json_schema"):
        schema = model.model_json_schema(ref_template="#/$defs/{model}")
    else:
        schema = model.schema(ref_template="#/$defs/{model}")
    nested = schema.pop("$defs", schema.pop("definitions", {}))
    definitions.update(nested)
    definitions.setdefault(model.__name__, schema)

print(json.dumps({"$schema": "https://json-schema.org/draft/2020-12/schema", "$defs": definitions}))
"##;

fn pydantic_json_schema(path: &Path) -> Result<Value> {
    let output = run_python(path)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Failed to extract Pydantic schemas from {}:\n{}",
            path.display(),
            stderr.trim()
        );
    }

    serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "Pydantic schema extractor returned invalid JSON for {}",
            path.display()
        )
    })
}

fn run_python(path: &Path) -> Result<Output> {
    if let Ok(python) = std::env::var("BAML_PYTHON_PATH") {
        return Command::new(&python)
            .arg("-c")
            .arg(PYDANTIC_SCHEMA_SCRIPT)
            .arg(path)
            .output()
            .with_context(|| format!("Failed to run Python interpreter {python}"));
    }

    let mut last_error = None;
    for python in ["python3", "python"] {
        match Command::new(python)
            .arg("-c")
            .arg(PYDANTIC_SCHEMA_SCRIPT)
            .arg(path)
            .output()
        {
            Ok(output) => return Ok(output),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => last_error = Some(error),
            Err(error) => return Err(error).with_context(|| format!("Failed to run {python}")),
        }
    }

    Err(last_error.unwrap_or_else(|| std::io::Error::other("Python interpreter not found")))
        .context("Pydantic import requires python3 or python on PATH")
}

pub fn json_schema_to_baml(schema: &Value, fallback_name: &str) -> Result<String> {
    Converter::new(schema).convert(fallback_name)
}

struct Converter<'a> {
    root: &'a Value,
    declarations: Vec<String>,
    ref_names: HashMap<String, String>,
    used_type_names: HashSet<String>,
    emitted: HashSet<String>,
}

impl<'a> Converter<'a> {
    fn new(root: &'a Value) -> Self {
        Self {
            root,
            declarations: Vec::new(),
            ref_names: HashMap::new(),
            used_type_names: HashSet::new(),
            emitted: HashSet::new(),
        }
    }

    fn convert(mut self, fallback_name: &str) -> Result<String> {
        let root = self
            .root
            .as_object()
            .context("JSON Schema must be a JSON object")?;

        let root_name = if has_root_schema(root) {
            let requested_name = root
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(fallback_name);
            let name = self.unique_type_name(requested_name);
            self.ref_names.insert("#".to_string(), name.clone());
            Some(name)
        } else {
            None
        };

        self.register_definitions(root, "$defs");
        self.register_definitions(root, "definitions");
        self.emit_definitions(root, "$defs")?;
        self.emit_definitions(root, "definitions")?;

        if let Some(name) = root_name {
            self.emit_named_schema(self.root, &name)?;
        }

        if self.declarations.is_empty() {
            bail!("JSON Schema did not contain a root schema, $defs, or definitions");
        }

        Ok(format!("{}\n", self.declarations.join("\n\n")))
    }

    fn register_definitions(&mut self, root: &Map<String, Value>, keyword: &str) {
        let Some(definitions) = root.get(keyword).and_then(Value::as_object) else {
            return;
        };
        for key in definitions.keys() {
            let reference = format!("#/{keyword}/{}", escape_json_pointer_segment(key));
            if !self.ref_names.contains_key(&reference) {
                let name = self.unique_type_name(key);
                self.ref_names.insert(reference, name);
            }
        }
    }

    fn emit_definitions(&mut self, root: &Map<String, Value>, keyword: &str) -> Result<()> {
        let Some(definitions) = root.get(keyword).and_then(Value::as_object) else {
            return Ok(());
        };
        for (key, schema) in definitions {
            let reference = format!("#/{keyword}/{}", escape_json_pointer_segment(key));
            let name = self
                .ref_names
                .get(&reference)
                .expect("definitions were registered before emission")
                .clone();
            self.emit_named_schema(schema, &name)?;
        }
        Ok(())
    }

    fn emit_named_schema(&mut self, schema: &Value, name: &str) -> Result<()> {
        if !self.emitted.insert(name.to_string()) {
            return Ok(());
        }

        if is_string_enum(schema) {
            self.emit_enum(schema, name)?;
        } else if is_object_schema(schema) && !is_map_schema(schema) {
            self.emit_class(schema, name)?;
        } else {
            let ty = self.schema_type(schema, name)?;
            self.declarations.push(format!("type {name} = {ty}"));
        }
        Ok(())
    }

    fn emit_enum(&mut self, schema: &Value, name: &str) -> Result<()> {
        let values = schema
            .get("enum")
            .and_then(Value::as_array)
            .context("enum must be an array")?;
        if values.is_empty() {
            bail!("Enum {name} cannot be empty");
        }

        let mut used = HashSet::new();
        let mut lines = Vec::with_capacity(values.len());
        for value in values {
            let raw = value.as_str().context("BAML enums require string values")?;
            let base = type_identifier(raw);
            let identifier = unique_identifier(&base, &mut used);
            let alias = (identifier != raw)
                .then(|| format!(" @alias({})", json_string(raw)))
                .unwrap_or_default();
            lines.push(format!("  {identifier}{alias}"));
        }
        self.declarations
            .push(format!("enum {name} {{\n{}\n}}", lines.join("\n")));
        Ok(())
    }

    fn emit_class(&mut self, schema: &Value, name: &str) -> Result<()> {
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let required: HashSet<&str> = schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();

        let mut used_fields = HashSet::new();
        let mut lines = Vec::with_capacity(properties.len());
        for (property_name, property_schema) in &properties {
            let suggested_name = format!("{name}{}", type_identifier(property_name));
            let mut ty = self.schema_type(property_schema, &suggested_name)?;
            if !required.contains(property_name.as_str()) {
                ty = optional_type(ty);
            }
            let base = field_identifier(property_name);
            let field_name = unique_identifier(&base, &mut used_fields);
            let alias = (field_name != *property_name)
                .then(|| format!(" @alias({})", json_string(property_name)))
                .unwrap_or_default();
            lines.push(format!("  {field_name} {ty}{alias}"));
        }

        self.declarations
            .push(format!("class {name} {{\n{}\n}}", lines.join("\n")));
        Ok(())
    }

    fn schema_type(&mut self, schema: &Value, suggested_name: &str) -> Result<String> {
        if schema.as_object().is_some_and(Map::is_empty) {
            return Ok("string | int | float | bool | null".to_string());
        }
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            return self.reference_type(reference);
        }
        if let Some(value) = schema.get("const") {
            return literal_type(value);
        }
        if schema.get("enum").is_some() {
            if is_string_enum(schema) {
                let name = self.unique_type_name(suggested_name);
                self.emit_named_schema(schema, &name)?;
                return Ok(name);
            }
            let values = schema["enum"].as_array().context("enum must be an array")?;
            return union_type(
                values
                    .iter()
                    .map(literal_type)
                    .collect::<Result<Vec<_>>>()?,
            );
        }
        if let Some(variants) = schema
            .get("anyOf")
            .or_else(|| schema.get("oneOf"))
            .and_then(Value::as_array)
        {
            let mut types = Vec::with_capacity(variants.len());
            for (index, variant) in variants.iter().enumerate() {
                types.push(
                    self.schema_type(variant, &format!("{suggested_name}Option{}", index + 1))?,
                );
            }
            let mut ty = union_type(types)?;
            if schema.get("nullable").and_then(Value::as_bool) == Some(true) {
                ty = optional_type(ty);
            }
            return Ok(ty);
        }

        let mut ty = match schema.get("type") {
            Some(Value::String(schema_type)) => {
                self.single_type(schema_type, schema, suggested_name)?
            }
            Some(Value::Array(schema_types)) => {
                let mut types = Vec::with_capacity(schema_types.len());
                for schema_type in schema_types {
                    let schema_type = schema_type
                        .as_str()
                        .context("JSON Schema type arrays must contain strings")?;
                    types.push(self.single_type(schema_type, schema, suggested_name)?);
                }
                union_type(types)?
            }
            Some(_) => bail!("JSON Schema type must be a string or an array of strings"),
            None if is_object_schema(schema) => self.object_type(schema, suggested_name)?,
            None => bail!("Unsupported JSON Schema without type, $ref, enum, or union"),
        };

        if schema.get("nullable").and_then(Value::as_bool) == Some(true) {
            ty = optional_type(ty);
        }
        Ok(ty)
    }

    fn single_type(
        &mut self,
        schema_type: &str,
        schema: &Value,
        suggested_name: &str,
    ) -> Result<String> {
        match schema_type {
            "string" => Ok("string".to_string()),
            "integer" => Ok("int".to_string()),
            "number" => Ok("float".to_string()),
            "boolean" => Ok("bool".to_string()),
            "null" => Ok("null".to_string()),
            "array" => {
                let item_schema = schema
                    .get("items")
                    .context("Array schemas must define items")?;
                let item_type = self.schema_type(item_schema, suggested_name)?;
                Ok(format!("{}[]", parenthesize_union(&item_type)))
            }
            "object" => self.object_type(schema, suggested_name),
            other => bail!("Unsupported JSON Schema type {other:?}"),
        }
    }

    fn object_type(&mut self, schema: &Value, suggested_name: &str) -> Result<String> {
        if is_map_schema(schema) {
            let value_type = match schema.get("additionalProperties") {
                Some(Value::Object(_)) => self.schema_type(
                    &schema["additionalProperties"],
                    &format!("{suggested_name}Value"),
                )?,
                Some(Value::Bool(false)) => "string".to_string(),
                _ => "string | int | float | bool | null".to_string(),
            };
            return Ok(format!("map<string, {}>", parenthesize_union(&value_type)));
        }

        let name = self.unique_type_name(suggested_name);
        self.emit_named_schema(schema, &name)?;
        Ok(name)
    }

    fn reference_type(&mut self, reference: &str) -> Result<String> {
        if let Some(name) = self.ref_names.get(reference) {
            return Ok(name.clone());
        }
        let pointer = reference.strip_prefix('#').with_context(|| {
            format!("Only local JSON Schema references are supported: {reference}")
        })?;
        let referenced_schema = self
            .root
            .pointer(pointer)
            .cloned()
            .with_context(|| format!("Unresolved JSON Schema reference {reference}"))?;
        let raw_name = pointer
            .rsplit('/')
            .next()
            .map(unescape_json_pointer_segment)
            .unwrap_or_else(|| "ReferencedType".to_string());
        let name = self.unique_type_name(&raw_name);
        self.ref_names.insert(reference.to_string(), name.clone());
        self.emit_named_schema(&referenced_schema, &name)?;
        Ok(name)
    }

    fn unique_type_name(&mut self, raw: &str) -> String {
        let base = type_identifier(raw);
        unique_identifier(&base, &mut self.used_type_names)
    }
}

fn has_root_schema(schema: &Map<String, Value>) -> bool {
    schema.keys().any(|key| {
        !matches!(
            key.as_str(),
            "$schema" | "$id" | "$defs" | "definitions" | "title" | "description"
        )
    })
}

fn is_object_schema(schema: &Value) -> bool {
    schema.get("type").and_then(Value::as_str) == Some("object")
        || schema.get("properties").is_some()
        || schema.get("additionalProperties").is_some()
}

fn is_map_schema(schema: &Value) -> bool {
    schema
        .get("properties")
        .and_then(Value::as_object)
        .is_none_or(Map::is_empty)
        && matches!(
            schema.get("additionalProperties"),
            Some(Value::Object(_)) | Some(Value::Bool(true))
        )
}

fn is_string_enum(schema: &Value) -> bool {
    schema
        .get("enum")
        .and_then(Value::as_array)
        .is_some_and(|values| !values.is_empty() && values.iter().all(Value::is_string))
}

fn union_type(types: Vec<String>) -> Result<String> {
    let mut seen = HashSet::new();
    let types: Vec<_> = types
        .into_iter()
        .filter(|ty| seen.insert(ty.clone()))
        .collect();
    if types.is_empty() {
        bail!("JSON Schema union cannot be empty");
    }
    Ok(types.join(" | "))
}

fn optional_type(ty: String) -> String {
    if ty == "null" || ty.ends_with('?') || ty.split(" | ").any(|part| part == "null") {
        ty
    } else if ty.contains(" | ") {
        format!("({ty})?")
    } else {
        format!("{ty}?")
    }
}

fn parenthesize_union(ty: &str) -> String {
    if ty.contains(" | ") && !(ty.starts_with('(') && ty.ends_with(')')) {
        format!("({ty})")
    } else {
        ty.to_string()
    }
}

fn literal_type(value: &Value) -> Result<String> {
    match value {
        Value::String(value) => Ok(json_string(value)),
        Value::Number(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Null => Ok("null".to_string()),
        Value::Array(_) | Value::Object(_) => {
            bail!("BAML does not support array or object literals")
        }
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a string cannot fail")
}

fn type_identifier(value: &str) -> String {
    let mut identifier = String::new();
    let mut capitalize = true;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if capitalize {
                identifier.push(character.to_ascii_uppercase());
                capitalize = false;
            } else {
                identifier.push(character);
            }
        } else {
            capitalize = true;
        }
    }
    if identifier.is_empty() {
        identifier.push_str("ImportedType");
    }
    if identifier.starts_with(|character: char| character.is_ascii_digit()) {
        identifier.insert_str(0, "Type");
    }
    if identifier == "BamlClient" || baml_keywords().contains(identifier.as_str()) {
        identifier.push('_');
    }
    identifier
}

fn field_identifier(value: &str) -> String {
    let mut identifier: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if identifier.is_empty() {
        identifier.push_str("field");
    }
    if identifier.starts_with(|character: char| character.is_ascii_digit()) {
        identifier.insert_str(0, "field_");
    }
    if PYTHON_KEYWORDS.contains(&identifier.as_str())
        || baml_keywords().contains(identifier.as_str())
    {
        identifier.push('_');
    }
    identifier
}

const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

fn unique_identifier(base: &str, used: &mut HashSet<String>) -> String {
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{base}{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn escape_json_pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

fn unescape_json_pointer_segment(segment: &str) -> String {
    segment.replace("~1", "/").replace("~0", "~")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use internal_baml_core::{internal_baml_diagnostics::SourceFile, FeatureFlags};
    use serde_json::json;

    use super::json_schema_to_baml;

    #[test]
    fn converts_nested_recursive_schema_to_valid_baml() {
        let schema = json!({
            "title": "Resume",
            "type": "object",
            "required": ["name", "education", "status", "contact"],
            "properties": {
                "name": {"type": "string"},
                "education": {"type": "array", "items": {"$ref": "#/$defs/Education"}},
                "status": {"$ref": "#/$defs/Status"},
                "contact": {
                    "anyOf": [
                        {"type": "string"},
                        {"type": "object", "required": ["phone-number"], "properties": {
                            "phone-number": {"type": "string"}
                        }}
                    ]
                },
                "metadata": {"type": "object", "additionalProperties": {"type": "integer"}}
            },
            "$defs": {
                "Education": {
                    "type": "object",
                    "required": ["school"],
                    "properties": {"school": {"type": "string"}, "year": {"type": ["integer", "null"]}}
                },
                "Status": {"type": "string", "enum": ["in-progress", "complete"]},
                "TreeNode": {
                    "type": "object",
                    "required": ["value"],
                    "properties": {
                        "value": {"type": "string"},
                        "children": {"type": "array", "items": {"$ref": "#/$defs/TreeNode"}}
                    }
                }
            }
        });

        let baml = json_schema_to_baml(&schema, "schema").expect("schema should convert");
        assert!(baml.contains("enum Status"));
        assert!(baml.contains("children TreeNode[]?"));
        assert!(baml.contains("metadata map<string, int>?"));
        assert!(baml.contains("phone_number string @alias(\"phone-number\")"));
        assert_valid_baml(&baml);
    }

    #[test]
    fn converts_a_top_level_union_and_numeric_literals() {
        let schema = json!({
            "title": "Result",
            "oneOf": [
                {"type": "string"},
                {"enum": [1, 2, 3]},
                {"type": "null"}
            ]
        });
        let baml = json_schema_to_baml(&schema, "schema").expect("schema should convert");
        assert_eq!(baml, "type Result = string | 1 | 2 | 3 | null\n");
        assert_valid_baml(&baml);
    }

    #[test]
    fn escapes_baml_reserved_identifiers() {
        let schema = json!({
            "title": "Self",
            "type": "object",
            "required": ["class", "enum", "function", "true", "int"],
            "properties": {
                "class": {"type": "string"},
                "enum": {"type": "string"},
                "function": {"type": "string"},
                "true": {"type": "boolean"},
                "int": {"type": "integer"}
            }
        });

        let baml = json_schema_to_baml(&schema, "schema").expect("schema should convert");
        assert!(baml.contains("class Self_"));
        for field in ["class", "enum", "function", "true", "int"] {
            assert!(baml.contains(&format!("{field}_ ")));
            assert!(baml.contains(&format!("@alias(\"{field}\")")));
        }
        assert_valid_baml(&baml);
    }

    #[test]
    fn resolves_a_root_recursive_reference_to_the_root_class() {
        let schema = json!({
            "title": "Node",
            "type": "object",
            "required": ["value"],
            "properties": {
                "value": {"type": "string"},
                "children": {"type": "array", "items": {"$ref": "#"}}
            }
        });
        let baml = json_schema_to_baml(&schema, "schema").expect("schema should convert");
        assert!(baml.contains("children Node[]?"));
        assert!(!baml.contains("ImportedType"));
        assert_valid_baml(&baml);
    }

    fn assert_valid_baml(baml: &str) {
        let path = PathBuf::from("generated.baml");
        let validated = internal_baml_core::validate(
            PathBuf::from(".").as_path(),
            vec![SourceFile::from((path, baml.to_string()))],
            FeatureFlags::new(),
        );
        assert!(
            !validated.diagnostics.has_errors(),
            "generated BAML should compile:\n{}\n{:#?}",
            baml,
            validated.diagnostics
        );
    }
}
