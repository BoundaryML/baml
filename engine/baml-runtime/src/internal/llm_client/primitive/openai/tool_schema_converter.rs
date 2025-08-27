use baml_types::{TypeIR, TypeValue, LiteralValue};
use baml_types::ir_type::UnionConstructor;
use internal_baml_core::ir::{
    repr::IntermediateRepr,
    IRHelper,
};
use serde_json::{json, Value};
use crate::internal::llm_client::primitive::openai::types::{Tool, ToolFunction};
use crate::{RuntimeContext, BamlSrcReader};
use std::collections::HashMap;

pub struct ToolSchemaConverter<'a> {
    definitions: HashMap<String, Value>,
    ir: &'a IntermediateRepr,
    ctx: &'a RuntimeContext,
}

impl<'a> ToolSchemaConverter<'a> {
    pub fn new(ir: &'a IntermediateRepr, ctx: &'a RuntimeContext) -> Self {
        Self {
            definitions: HashMap::new(),
            ir,
            ctx,
        }
    }
    
    pub fn convert_return_type_to_tools(&mut self, return_type: &TypeIR) -> Vec<Tool> {
        match return_type {
            TypeIR::Class { name, .. } => {
                vec![self.create_tool_from_class(name)]
            }
            TypeIR::Union(union_type, _) => {
                let mut tools = Vec::new();
                for variant in union_type.iter_skip_null() {
                    if let Some(tool) = self.type_to_tool(variant) {
                        tools.push(tool);
                    }
                }
                tools
            }
            TypeIR::List(inner, _) => {
                // Array returns enable parallel tool calls
                self.convert_return_type_to_tools(inner)
            }
            _ => {
                // Primitive types get generic tool names
                vec![self.create_tool_from_primitive(return_type)]
            }
        }
    }
    
    fn create_tool_from_class(&mut self, class_name: &str) -> Tool {
        Tool {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: class_name.to_string(),
                description: None,  // TODO: Extract from class metadata
                parameters: self.class_to_json_schema(class_name),
                strict: None,  // TODO: Add configuration option for strict mode
            }
        }
    }
    
    fn create_tool_from_primitive(&mut self, type_ir: &TypeIR) -> Tool {
        let (name, schema) = match type_ir {
            TypeIR::Primitive(TypeValue::String, _) => ("string_value", json!({
                "type": "object",
                "properties": {
                    "value": {"type": "string"}
                },
                "required": ["value"],
                "additionalProperties": false
            })),
            TypeIR::Primitive(TypeValue::Int, _) => ("integer_value", json!({
                "type": "object",
                "properties": {
                    "value": {"type": "integer"}
                },
                "required": ["value"],
                "additionalProperties": false
            })),
            TypeIR::Primitive(TypeValue::Float, _) => ("number_value", json!({
                "type": "object",
                "properties": {
                    "value": {"type": "number"}
                },
                "required": ["value"],
                "additionalProperties": false
            })),
            TypeIR::Primitive(TypeValue::Bool, _) => ("boolean_value", json!({
                "type": "object",
                "properties": {
                    "value": {"type": "boolean"}
                },
                "required": ["value"],
                "additionalProperties": false
            })),
            _ => ("generic_value", json!({
                "type": "object",
                "properties": {
                    "value": {}
                },
                "required": ["value"],
                "additionalProperties": false
            })),
        };
        
        Tool {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: name.to_string(),
                description: None,
                parameters: schema,
                strict: None,
            }
        }
    }
    
    fn type_to_tool(&mut self, type_ir: &TypeIR) -> Option<Tool> {
        match type_ir {
            TypeIR::Class { name, .. } => Some(self.create_tool_from_class(name)),
            TypeIR::Primitive(TypeValue::Null, _) => None,  // Skip null types in unions
            _ => Some(self.create_tool_from_primitive(type_ir)),
        }
    }
    
    fn class_to_json_schema(&mut self, class_name: &str) -> Value {
        // Check if already cached
        if self.definitions.contains_key(class_name) {
            return json!({
                "type": "object",
                "properties": {},
                "$ref": format!("#/definitions/{}", class_name)
            });
        }
        
        // Look up the class definition from the context
        let class_walker = match self.ir.find_class(class_name) {
            Ok(walker) => walker,
            Err(_) => {
                // Class not found, return empty schema
                return json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                });
            }
        };
        
        // Convert each field to JSON Schema
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        
        // Use walk_fields() method to iterate over fields
        for field in class_walker.walk_fields() {
            let field_name = field.name();
            let field_type = field.r#type();
            
            properties.insert(field_name.to_string(), self.type_to_json_schema(&field_type));
            
            // Check if field is required (non-optional)
            if !self.is_optional(&field_type) {
                required.push(field_name.to_string());
            }
        }
        
        let schema = json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        });
        
        // Cache the schema
        self.definitions.insert(class_name.to_string(), schema.clone());
        
        schema
    }
    
    fn enum_to_json_schema(&mut self, enum_name: &str) -> Value {
        // Check if already cached
        if self.definitions.contains_key(enum_name) {
            return json!({"$ref": format!("#/definitions/{}", enum_name)});
        }
        
        // Look up the enum definition from the context
        let enum_walker = match self.ir.find_enum(enum_name) {
            Ok(walker) => walker,
            Err(_) => {
                // Enum not found, return string schema
                return json!({"type": "string"});
            }
        };
        
        // Use walk_values() method to get enum values
        let mut values = Vec::new();
        for value in enum_walker.walk_values() {
            values.push(value.name().to_string());
        }
        
        let schema = json!({
            "type": "string",
            "enum": values
        });
        
        // Cache the schema
        self.definitions.insert(enum_name.to_string(), schema.clone());
        
        schema
    }
    
    fn type_to_json_schema(&mut self, type_ir: &TypeIR) -> Value {
        match type_ir {
            TypeIR::Primitive(TypeValue::String, _) => json!({"type": "string"}),
            TypeIR::Primitive(TypeValue::Int, _) => json!({"type": "integer"}),
            TypeIR::Primitive(TypeValue::Float, _) => json!({"type": "number"}),
            TypeIR::Primitive(TypeValue::Bool, _) => json!({"type": "boolean"}),
            TypeIR::Primitive(TypeValue::Null, _) => json!({"type": "null"}),
            TypeIR::Primitive(TypeValue::Media(_), _) => {
                // Media types cannot be used as return types
                json!({
                    "type": "string",
                    "description": "Media type (not supported as return type)"
                })
            }
            TypeIR::Class { name, .. } => {
                self.class_to_json_schema(name)
            }
            TypeIR::Enum { name, .. } => {
                self.enum_to_json_schema(name)
            }
            TypeIR::List(inner, _) => {
                json!({
                    "type": "array",
                    "items": self.type_to_json_schema(inner)
                })
            }
            TypeIR::Map(key, value, _) => {
                // Maps in JSON Schema are objects with additionalProperties
                json!({
                    "type": "object",
                    "additionalProperties": self.type_to_json_schema(value)
                })
            }
            TypeIR::Union(union_type, _) => {
                let mut schemas = Vec::new();
                for t in union_type.iter_skip_null() {
                    schemas.push(self.type_to_json_schema(t));
                }
                
                // Check if this is actually an optional type (T | null)
                if union_type.is_optional() && schemas.len() == 1 {
                    // Optional type - use array notation
                    let mut types = vec![schemas[0].clone()];
                    types.push(json!("null"));
                    json!({"type": types})
                } else if schemas.is_empty() {
                    // Just null
                    json!({"type": "null"})
                } else if schemas.len() == 1 {
                    // Single type
                    schemas[0].clone()
                } else {
                    // True union - use anyOf
                    json!({"anyOf": schemas})
                }
            }
            TypeIR::Tuple(types, _) => {
                let items: Vec<Value> = types.iter()
                    .map(|t| self.type_to_json_schema(t))
                    .collect();
                json!({
                    "type": "array",
                    "items": items,
                    "minItems": types.len(),
                    "maxItems": types.len()
                })
            }
            TypeIR::RecursiveTypeAlias { name, .. } => {
                self.handle_recursive_type_alias(name)
            }
            TypeIR::Arrow { .. } => {
                // Function types cannot be converted to JSON Schema
                json!({
                    "type": "object",
                    "description": "Function type (not supported in JSON Schema)"
                })
            }
            TypeIR::Literal(literal_value, _) => {
                match literal_value {
                    LiteralValue::String(s) => json!({"const": s}),
                    LiteralValue::Int(i) => json!({"const": i}),
                    LiteralValue::Bool(b) => json!({"const": b}),
                }
            }
        }
    }
    
    fn handle_recursive_type_alias(&mut self, alias_name: &str) -> Value {
        // Check if we're already processing this alias (circular reference)
        if self.definitions.contains_key(alias_name) {
            return json!({"$ref": format!("#/definitions/{}", alias_name)});
        }
        
        // Look up the type alias definition from the IR context
        let type_alias_walker = match self.ir.find_type_alias(alias_name) {
            Ok(walker) => walker,
            Err(_) => {
                // Type alias not found, return any type
                return json!({});
            }
        };
        
        // For recursive aliases, we need to create a definition
        // Mark as being processed to break potential cycles
        self.definitions.insert(alias_name.to_string(), json!({"type": "object"}));
        
        // Get the resolved type from the type alias
        // We need to use repr() to get the TypeAlias, then access its r#type field
        // However, repr() requires a ParserDatabase which we don't have access to here
        // For now, just return a reference to the alias since we can't resolve it
        json!({"$ref": format!("#/definitions/{}", alias_name)})
    }
    
    fn is_optional(&self, type_ir: &TypeIR) -> bool {
        match type_ir {
            TypeIR::Union(union_type, _) => union_type.is_optional(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod test_tool_schema {
    use super::*;
    use baml_types::{TypeIR, TypeValue};
    use internal_baml_core::ir::repr::IntermediateRepr;
    
    fn create_test_context() -> (IntermediateRepr, RuntimeContext) {
        // Create minimal test IR and context
        let ir = IntermediateRepr::create_empty();
        let ctx = RuntimeContext::new(
            std::sync::Arc::new(None), // BamlSrcReader is an Option
            std::collections::HashMap::new(),
            std::collections::HashMap::new(),
            None,
            indexmap::IndexMap::new(),
            indexmap::IndexMap::new(),
            indexmap::IndexMap::new(),
            vec![],
            vec![],
            vec![],
        );
        (ir, ctx)
    }

    #[test]
    fn test_primitive_type_to_tool() {
        let (ir, ctx) = create_test_context();
        let mut converter = ToolSchemaConverter::new(&ir, &ctx);
        
        // Test string type
        let string_type = TypeIR::string();
        let tools = converter.convert_return_type_to_tools(&string_type);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.name, "string_value");
        assert_eq!(tools[0].tool_type, "function");
        
        // Verify the schema structure
        let params = &tools[0].function.parameters;
        assert!(params["properties"]["value"]["type"].as_str() == Some("string"));
        assert!(params["required"].as_array().unwrap().contains(&json!("value")));
    }

    #[test]
    fn test_integer_type_to_tool() {
        let (ir, ctx) = create_test_context();
        let mut converter = ToolSchemaConverter::new(&ir, &ctx);
        
        let int_type = TypeIR::int();
        let tools = converter.convert_return_type_to_tools(&int_type);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.name, "integer_value");
        
        let params = &tools[0].function.parameters;
        assert!(params["properties"]["value"]["type"].as_str() == Some("integer"));
    }

    #[test]
    fn test_list_type_to_tools() {
        let (ir, ctx) = create_test_context();
        let mut converter = ToolSchemaConverter::new(&ir, &ctx);
        
        // List of strings should produce tool for string (enables parallel calls)
        let list_type = TypeIR::list(TypeIR::string());
        let tools = converter.convert_return_type_to_tools(&list_type);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.name, "string_value");
    }

    #[test]
    fn test_union_type_to_multiple_tools() {
        let (ir, ctx) = create_test_context();
        let mut converter = ToolSchemaConverter::new(&ir, &ctx);
        
        // Union of string and int should produce two tools
        let union_type = TypeIR::union(vec![TypeIR::string(), TypeIR::int()]);
        let tools = converter.convert_return_type_to_tools(&union_type);
        assert_eq!(tools.len(), 2);
        
        let tool_names: Vec<String> = tools.iter().map(|t| t.function.name.clone()).collect();
        assert!(tool_names.contains(&"string_value".to_string()));
        assert!(tool_names.contains(&"integer_value".to_string()));
    }

    #[test]
    fn test_optional_type_schema() {
        let (ir, ctx) = create_test_context();
        let mut converter = ToolSchemaConverter::new(&ir, &ctx);
        
        // Optional string (string | null)
        let optional_type = TypeIR::union(vec![TypeIR::string(), TypeIR::null()]);
        let schema = converter.type_to_json_schema(&optional_type);
        
        // Should produce type array notation for optional
        assert!(schema["type"].is_array());
        let types = schema["type"].as_array().unwrap();
        assert!(types.contains(&json!("string")));
        assert!(types.contains(&json!("null")));
    }

    #[test]
    fn test_map_type_schema() {
        let (ir, ctx) = create_test_context();
        let mut converter = ToolSchemaConverter::new(&ir, &ctx);
        
        // Map<string, int>
        let map_type = TypeIR::map(TypeIR::string(), TypeIR::int());
        let schema = converter.type_to_json_schema(&map_type);
        
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"]["type"], "integer");
    }

    #[test]
    fn test_tuple_type_schema() {
        let (ir, ctx) = create_test_context();
        let mut converter = ToolSchemaConverter::new(&ir, &ctx);
        
        // Tuple of [string, int, bool]
        let tuple_type = TypeIR::tuple(vec![TypeIR::string(), TypeIR::int(), TypeIR::bool()]);
        let schema = converter.type_to_json_schema(&tuple_type);
        
        assert_eq!(schema["type"], "array");
        assert_eq!(schema["minItems"], 3);
        assert_eq!(schema["maxItems"], 3);
        
        let items = schema["items"].as_array().unwrap();
        assert_eq!(items[0]["type"], "string");
        assert_eq!(items[1]["type"], "integer");
        assert_eq!(items[2]["type"], "boolean");
    }

    #[test]
    fn test_literal_type_schema() {
        let (ir, ctx) = create_test_context();
        let mut converter = ToolSchemaConverter::new(&ir, &ctx);
        
        // String literal "success"
        let literal_type = TypeIR::literal("success");
        let schema = converter.type_to_json_schema(&literal_type);
        assert_eq!(schema["const"], "success");
        
        // Int literal 42
        let int_literal = TypeIR::literal(42i64);
        let schema = converter.type_to_json_schema(&int_literal);
        assert_eq!(schema["const"], 42);
        
        // Bool literal true
        let bool_literal = TypeIR::literal(true);
        let schema = converter.type_to_json_schema(&bool_literal);
        assert_eq!(schema["const"], true);
    }

    #[test]
    fn test_media_type_schema() {
        let (ir, ctx) = create_test_context();
        let mut converter = ToolSchemaConverter::new(&ir, &ctx);
        
        // Media types should get special handling
        let media_type = TypeIR::Primitive(
            TypeValue::Media(baml_types::BamlMediaType::Image),
            Default::default()
        );
        let schema = converter.type_to_json_schema(&media_type);
        
        assert_eq!(schema["type"], "string");
        assert!(schema["description"].as_str().unwrap().contains("Media type"));
    }
}