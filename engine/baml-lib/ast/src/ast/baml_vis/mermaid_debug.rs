use std::collections::HashMap;

use super::header_collector::{HeaderCollector, HeaderIndex};
use crate::ast::{
    traits::{WithIdentifier, WithName},
    Argument, ArgumentsList, Assignment, Ast, Attribute, BlockArgs, ClassConstructor,
    ClassConstructorField, ExprFn, Expression, ExpressionBlock, Field, FieldType, Header, Stmt,
    TemplateString, Top, TopLevelAssignment, TypeExpressionBlock, ValueExprBlock,
};

/// A debug utility for converting AST structures to Mermaid diagrams
#[derive(Debug)]
pub struct MermaidDiagramGenerator {
    /// Counter for generating unique node IDs
    node_counter: u32,
    /// Map from node addresses to generated IDs for deduplication
    node_ids: HashMap<String, String>,
    /// The accumulated Mermaid diagram content
    content: Vec<String>,
    /// Whether to include CSS styling for better visual appearance
    use_styling: bool,
}

impl MermaidDiagramGenerator {
    pub fn new() -> Self {
        Self::new_with_styling(true)
    }

    /// Create a new generator with optional styling
    pub fn new_with_styling(use_styling: bool) -> Self {
        let mut content = vec!["graph TD".to_string()];

        if use_styling {
            content.extend_from_slice(&[
                "    %% Styling for better visual appearance".to_string(),
                "    classDef functionNode fill:#e1f5fe,stroke:#01579b,stroke-width:2px,color:#000".to_string(),
                "    classDef headerNode fill:#f3e5f5,stroke:#4a148c,stroke-width:3px,color:#000,font-weight:bold".to_string(),
                "    classDef fieldNode fill:#e8f5e8,stroke:#1b5e20,stroke-width:1px,color:#000".to_string(),
                "    classDef statementNode fill:#fff3e0,stroke:#e65100,stroke-width:1px,color:#000".to_string(),
                "    classDef expressionNode fill:#fce4ec,stroke:#880e4f,stroke-width:1px,color:#000".to_string(),
                "    classDef valueNode fill:#f1f8e9,stroke:#33691e,stroke-width:1px,color:#000".to_string(),
                "    classDef typeNode fill:#e0f2f1,stroke:#004d40,stroke-width:1px,color:#000".to_string(),
                "    classDef attributeNode fill:#fafafa,stroke:#424242,stroke-width:1px,color:#000".to_string(),
                "    %% Link styling".to_string(),
                "    linkStyle default stroke:#666,stroke-width:2px".to_string(),
                "".to_string(),
            ]);
        }

        Self {
            node_counter: 0,
            node_ids: HashMap::new(),
            content,
            use_styling,
        }
    }

    /// Generate a Mermaid diagram for the entire AST
    pub fn generate_ast_diagram(ast: &Ast) -> String {
        Self::generate_ast_diagram_with_styling(ast, true)
    }

    /// Generate a Mermaid diagram for the entire AST with optional styling
    pub fn generate_ast_diagram_with_styling(ast: &Ast, use_styling: bool) -> String {
        let mut generator = Self::new_with_styling(use_styling);
        // dbg!(&ast);
        generator.visit_ast(ast);
        generator.content.join("\n")
    }

    /// Generate a Mermaid diagram that visualizes only the headers from the AST
    pub fn generate_headers_diagram(ast: &Ast) -> String {
        Self::generate_headers_diagram_with_styling(ast, true)
    }

    /// Generate a Mermaid diagram that visualizes only the headers from the AST with optional styling
    pub fn generate_headers_diagram_with_styling(ast: &Ast, use_styling: bool) -> String {
        let mut generator = Self::new_with_styling(use_styling);
        generator.content = vec!["graph TD".to_string()];
        generator.render_headers_only(ast);
        generator.content.join("\n")
    }

    /// Get a unique node ID for a given pointer/key
    fn get_node_id(&mut self, key: &str, label: &str) -> String {
        if let Some(id) = self.node_ids.get(key) {
            id.clone()
        } else {
            let id = format!("n{}", self.node_counter);
            self.node_counter += 1;
            self.node_ids.insert(key.to_string(), id.clone());

            // Escape quotes and special characters in label for Mermaid
            let escaped_label = if self.use_styling {
                label
                    .replace('"', "&quot;")
                    .replace('\n', "<br/>")
                    .replace('(', "&#40;")
                    .replace(')', "&#41;")
            } else {
                label.replace('"', "\\\"").replace('\n', "\\n")
            };

            self.content
                .push(format!("    {id}[\"{escaped_label}\"]; "));
            id
        }
    }

    /// Get a unique node ID with CSS class styling
    fn get_node_id_with_class(&mut self, key: &str, label: &str, css_class: &str) -> String {
        if let Some(id) = self.node_ids.get(key) {
            id.clone()
        } else {
            let id = format!("n{}", self.node_counter);
            self.node_counter += 1;
            self.node_ids.insert(key.to_string(), id.clone());

            // Escape quotes and special characters in label for Mermaid
            let escaped_label = if self.use_styling {
                label
                    .replace('"', "&quot;")
                    .replace('\n', "<br/>")
                    .replace('(', "&#40;")
                    .replace(')', "&#41;")
            } else {
                label.replace('"', "\\\"").replace('\n', "\\n")
            };

            self.content
                .push(format!("    {id}[\"{escaped_label}\"]; "));

            // Apply CSS class only if styling is enabled
            if self.use_styling {
                self.content.push(format!("    class {id} {css_class};"));
            }
            id
        }
    }

    /// Connect two nodes in the diagram
    fn connect(&mut self, from: &str, to: &str, label: Option<&str>) {
        if let Some(label) = label {
            let escaped_label = label.replace('"', "&quot;");
            if self.use_styling {
                self.content.push(format!(
                    "    {from} -->|\"<b style='color:#4a148c'>{escaped_label}</b>\"| {to};"
                ));
            } else {
                self.content
                    .push(format!("    {from} -->|\"{escaped_label}\"| {to};"));
            }
        } else {
            self.content.push(format!("    {from} --> {to};"));
        }
    }

    /// Visit the root AST node
    fn visit_ast(&mut self, ast: &Ast) {
        let ast_key = format!("ast_{ast:p}");
        let ast_id = self.get_node_id_with_class(&ast_key, "AST Root", "typeNode");

        for (idx, top) in ast.tops.iter().enumerate() {
            let top_id = self.visit_top(top, idx);
            self.connect(&ast_id, &top_id, Some("contains"));
        }
    }

    /// Visit a top-level AST node
    fn visit_top(&mut self, top: &Top, index: usize) -> String {
        let top_key = format!("top_{index}_{top:p}");

        match top {
            Top::Enum(type_expr) => {
                let label = format!("Enum: {}", type_expr.identifier().name());
                let top_id = self.get_node_id_with_class(&top_key, &label, "typeNode");
                let type_expr_id = self.visit_type_expression_block(type_expr);
                self.connect(&top_id, &type_expr_id, None);
                top_id
            }
            Top::Class(type_expr) => {
                let label = format!("Class: {}", type_expr.identifier().name());
                let top_id = self.get_node_id_with_class(&top_key, &label, "typeNode");
                let type_expr_id = self.visit_type_expression_block(type_expr);
                self.connect(&top_id, &type_expr_id, None);
                top_id
            }
            Top::Function(value_expr) => {
                let label = format!("Function: {}", value_expr.identifier().name());
                let top_id = self.get_node_id_with_class(&top_key, &label, "functionNode");
                let value_expr_id = self.visit_value_expression_block(value_expr);
                self.connect(&top_id, &value_expr_id, None);
                top_id
            }
            Top::TypeAlias(assignment) => {
                let label = format!("TypeAlias: {}", assignment.identifier().name());
                let top_id = self.get_node_id_with_class(&top_key, &label, "typeNode");
                let assignment_id = self.visit_assignment(assignment);
                self.connect(&top_id, &assignment_id, None);
                top_id
            }
            Top::Client(value_expr) => {
                let label = format!("Client: {}", value_expr.identifier().name());
                let top_id = self.get_node_id_with_class(&top_key, &label, "functionNode");
                let value_expr_id = self.visit_value_expression_block(value_expr);
                self.connect(&top_id, &value_expr_id, None);
                top_id
            }
            Top::TemplateString(template) => {
                let label = format!("TemplateString: {}", template.identifier().name());
                let top_id = self.get_node_id_with_class(&top_key, &label, "expressionNode");
                let template_id = self.visit_template_string(template);
                self.connect(&top_id, &template_id, None);
                top_id
            }
            Top::Generator(value_expr) => {
                let label = format!("Generator: {}", value_expr.identifier().name());
                let top_id = self.get_node_id_with_class(&top_key, &label, "functionNode");
                let value_expr_id = self.visit_value_expression_block(value_expr);
                self.connect(&top_id, &value_expr_id, None);
                top_id
            }
            Top::TestCase(value_expr) => {
                let label = format!("TestCase: {}", value_expr.identifier().name());
                let top_id = self.get_node_id_with_class(&top_key, &label, "functionNode");
                let value_expr_id = self.visit_value_expression_block(value_expr);
                self.connect(&top_id, &value_expr_id, None);
                top_id
            }
            Top::RetryPolicy(value_expr) => {
                let label = format!("RetryPolicy: {}", value_expr.identifier().name());
                let top_id = self.get_node_id_with_class(&top_key, &label, "functionNode");
                let value_expr_id = self.visit_value_expression_block(value_expr);
                self.connect(&top_id, &value_expr_id, None);
                top_id
            }
            Top::TopLevelAssignment(assignment) => {
                let label = format!("Top::Assignment: {}", assignment.stmt.identifier.name());
                let top_id = self.get_node_id_with_class(&top_key, &label, "statementNode");
                let assignment_id = self.visit_top_level_assignment(assignment);
                self.connect(&top_id, &assignment_id, None);
                top_id
            }
            Top::ExprFn(expr_fn) => {
                let label = format!("Top::ExprFn: {}", expr_fn.name.name());
                let top_id = self.get_node_id_with_class(&top_key, &label, "functionNode");
                let expr_fn_id = self.visit_expr_fn(expr_fn);
                self.connect(&top_id, &expr_fn_id, None);
                top_id
            }
        }
    }

    /// Visit a type expression block (enum or class)
    fn visit_type_expression_block(&mut self, type_expr: &TypeExpressionBlock) -> String {
        let key = format!("type_expr_{type_expr:p}");
        let label = format!("TypeExpr<br/>name: {}", type_expr.name.name());
        let type_expr_id = self.get_node_id_with_class(&key, &label, "typeNode");

        // Visit input arguments if present
        if let Some(input) = &type_expr.input {
            let input_id = self.visit_block_args(input);
            self.connect(&type_expr_id, &input_id, Some("input"));
        }

        // Visit fields
        for (idx, field) in type_expr.fields.iter().enumerate() {
            let field_id = self.visit_field_type(field, idx);
            self.connect(&type_expr_id, &field_id, Some("field"));
        }

        // Visit attributes
        for (idx, attr) in type_expr.attributes.iter().enumerate() {
            let attr_id = self.visit_attribute(attr, idx);
            self.connect(&type_expr_id, &attr_id, Some("attr"));
        }

        type_expr_id
    }

    /// Visit a value expression block (function, client, etc.)
    fn visit_value_expression_block(&mut self, value_expr: &ValueExprBlock) -> String {
        let key = format!("value_expr_{value_expr:p}");
        let label = format!(
            "ValueExpr<br/>name: {}<br/>type: {:?}",
            value_expr.name.name(),
            value_expr.block_type
        );
        let value_expr_id = self.get_node_id_with_class(&key, &label, "functionNode");

        // Visit annotations
        for (idx, annotation) in value_expr.annotations.iter().enumerate() {
            let annotation_id = self.visit_header(annotation, idx);
            self.connect(&value_expr_id, &annotation_id, Some("annotation"));
        }

        // Visit input arguments if present
        if let Some(input) = &value_expr.input {
            let input_id = self.visit_block_args(input);
            self.connect(&value_expr_id, &input_id, Some("input"));
        }

        // Visit fields (expressions)
        for (idx, field) in value_expr.fields.iter().enumerate() {
            let field_id = self.visit_field_expression(field, idx);
            self.connect(&value_expr_id, &field_id, Some("field"));
        }

        // Visit attributes
        for (idx, attr) in value_expr.attributes.iter().enumerate() {
            let attr_id = self.visit_attribute(attr, idx);
            self.connect(&value_expr_id, &attr_id, Some("attr"));
        }

        value_expr_id
    }

    /// Visit block arguments
    fn visit_block_args(&mut self, block_args: &BlockArgs) -> String {
        let key = format!("block_args_{block_args:p}");
        let label = "BlockArgs";
        let block_args_id = self.get_node_id(&key, label);

        for (idx, (identifier, block_arg)) in block_args.args.iter().enumerate() {
            let arg_key = format!("block_arg_{idx}_{block_arg:p}");
            let arg_label = format!(
                "Arg: {}<br/>type: {}",
                identifier.name(),
                block_arg.field_type.name()
            );
            let arg_id = self.get_node_id(&arg_key, &arg_label);
            self.connect(&block_args_id, &arg_id, None);
        }

        block_args_id
    }

    /// Visit a field with FieldType
    fn visit_field_type(&mut self, field: &Field<FieldType>, index: usize) -> String {
        let key = format!("field_type_{index}_{field:p}");
        let mut label = format!("Field: {}", field.name.name());

        if let Some(field_type) = &field.expr {
            label.push_str(&format!("<br/>type: {}", field_type.name()));
        }

        let field_id = self.get_node_id_with_class(&key, &label, "fieldNode");

        // Visit attributes
        for (idx, attr) in field.attributes.iter().enumerate() {
            let attr_id = self.visit_attribute(attr, idx);
            self.connect(&field_id, &attr_id, Some("attr"));
        }

        field_id
    }

    /// Visit a field with Expression
    fn visit_field_expression(&mut self, field: &Field<Expression>, index: usize) -> String {
        let key = format!("field_expr_{index}_{field:p}");
        let label = format!("Field: {}", field.name.name());
        let field_id = self.get_node_id_with_class(&key, &label, "fieldNode");

        // Visit the expression if present
        if let Some(expr) = &field.expr {
            let expr_id = self.visit_expression(expr);
            self.connect(&field_id, &expr_id, Some("value"));
        }

        // Visit attributes
        for (idx, attr) in field.attributes.iter().enumerate() {
            let attr_id = self.visit_attribute(attr, idx);
            self.connect(&field_id, &attr_id, Some("attr"));
        }

        field_id
    }

    /// Visit an attribute
    fn visit_attribute(&mut self, attr: &Attribute, index: usize) -> String {
        let key = format!("attr_{index}_{attr:p}");
        let label = format!("@{}", attr.name.name());
        let attr_id = self.get_node_id_with_class(&key, &label, "attributeNode");

        // Visit arguments
        let args_id = self.visit_arguments_list(&attr.arguments);
        if !attr.arguments.arguments.is_empty() {
            self.connect(&attr_id, &args_id, Some("args"));
        }

        attr_id
    }

    /// Visit an arguments list
    fn visit_arguments_list(&mut self, args: &ArgumentsList) -> String {
        let key = format!("args_{args:p}");
        let label = "Arguments";
        let args_id = self.get_node_id(&key, label);

        for (idx, arg) in args.arguments.iter().enumerate() {
            let arg_id = self.visit_argument(arg, idx);
            self.connect(&args_id, &arg_id, None);
        }

        args_id
    }

    /// Visit an argument
    fn visit_argument(&mut self, arg: &Argument, index: usize) -> String {
        let key = format!("arg_{index}_{arg:p}");
        let label = "Argument";
        let arg_id = self.get_node_id(&key, label);

        let expr_id = self.visit_expression(&arg.value);
        self.connect(&arg_id, &expr_id, Some("value"));

        arg_id
    }

    /// Visit an expression
    fn visit_expression(&mut self, expr: &Expression) -> String {
        let key = format!("expr_{expr:p}");

        let expr_id = match expr {
            Expression::BoolValue(val, _) => {
                let label = format!("Bool: {val}");
                self.get_node_id_with_class(&key, &label, "valueNode")
            }
            Expression::NumericValue(val, _) => {
                let label = format!("Number: {val}");
                self.get_node_id_with_class(&key, &label, "valueNode")
            }
            Expression::StringValue(val, _) => {
                let label = format!("String: \"{}\"", val.chars().take(20).collect::<String>());
                self.get_node_id_with_class(&key, &label, "valueNode")
            }
            Expression::Identifier(ident) => {
                let label = format!("Identifier: {}", ident.name());
                self.get_node_id_with_class(&key, &label, "valueNode")
            }
            Expression::RawStringValue(raw) => {
                let preview = raw.value().chars().take(30).collect::<String>();
                let label = format!("RawString: \"{preview}...\"");
                self.get_node_id_with_class(&key, &label, "valueNode")
            }
            Expression::ClassConstructor(constructor, _) => {
                let label = "ClassConstructor".to_string();
                let expr_id = self.get_node_id_with_class(&key, &label, "expressionNode");
                let constructor_id = self.visit_class_constructor(constructor);
                self.connect(&expr_id, &constructor_id, Some("constructor"));
                expr_id
            }
            Expression::Array(exprs, _) => {
                let label = "Array".to_string();
                let expr_id = self.get_node_id_with_class(&key, &label, "expressionNode");
                for (idx, expr) in exprs.iter().enumerate() {
                    let child_id = self.visit_expression(expr);
                    self.connect(&expr_id, &child_id, Some(&format!("item_{idx}")));
                }
                expr_id
            }
            Expression::Map(map, _) => {
                let label = "Map".to_string();
                let expr_id = self.get_node_id_with_class(&key, &label, "expressionNode");
                for (idx, (key_expr, value_expr)) in map.iter().enumerate() {
                    let key_id = self.visit_expression(key_expr);
                    let value_id = self.visit_expression(value_expr);
                    self.connect(&expr_id, &key_id, Some(&format!("key_{idx}")));
                    self.connect(&expr_id, &value_id, Some(&format!("value_{idx}")));
                }
                expr_id
            }
            Expression::JinjaExpressionValue(jinja, _) => {
                let label = format!("Jinja: {jinja}");
                self.get_node_id(&key, &label)
            }
            Expression::Lambda(args, body, _) => {
                let label = "Lambda".to_string();
                let expr_id = self.get_node_id(&key, &label);
                let args_id = self.visit_arguments_list(args);
                let body_id = self.visit_expression_block(body);
                self.connect(&expr_id, &args_id, Some("args"));
                self.connect(&expr_id, &body_id, Some("body"));
                expr_id
            }
            Expression::App(app) => {
                let label = format!("App: {}", app.name.name());
                let expr_id = self.get_node_id(&key, &label);
                for (idx, arg) in app.args.iter().enumerate() {
                    let arg_id = self.visit_expression(arg);
                    self.connect(&expr_id, &arg_id, Some(&format!("arg_{idx}")));
                }
                expr_id
            }
            Expression::ExprBlock(block, _) => {
                let label = "ExprBlock".to_string();
                let expr_id = self.get_node_id(&key, &label);
                let block_id = self.visit_expression_block(block);
                self.connect(&expr_id, &block_id, Some("block"));
                expr_id
            }
            Expression::If(cond, then_expr, else_expr, _) => {
                let label = "If".to_string();
                let expr_id = self.get_node_id(&key, &label);
                let cond_id = self.visit_expression(cond);
                let then_id = self.visit_expression(then_expr);
                self.connect(&expr_id, &cond_id, Some("condition"));
                self.connect(&expr_id, &then_id, Some("then"));
                if let Some(else_expr) = else_expr {
                    let else_id = self.visit_expression(else_expr);
                    self.connect(&expr_id, &else_id, Some("else"));
                }
                expr_id
            }
            Expression::UnaryOperation { operator, expr, .. } => {
                let label = format!("Unary: {operator}");
                let expr_id = self.get_node_id(&key, &label);
                let child_id = self.visit_expression(expr);
                self.connect(&expr_id, &child_id, Some("expr"));
                expr_id
            }
            Expression::ArrayAccess(base, index, _) => {
                let label = "ArrayAccess".to_string();
                let expr_id = self.get_node_id_with_class(&key, &label, "expressionNode");
                let base_id = self.visit_expression(base);
                let index_id = self.visit_expression(index);
                self.connect(&expr_id, &base_id, Some("base"));
                self.connect(&expr_id, &index_id, Some("index"));
                expr_id
            }
            Expression::FieldAccess(base, field, _) => {
                let label = format!("FieldAccess: {}", field.name());
                let expr_id = self.get_node_id_with_class(&key, &label, "expressionNode");
                let base_id = self.visit_expression(base);
                self.connect(&expr_id, &base_id, Some("base"));
                expr_id
            }
            Expression::BinaryOperation {
                left,
                operator,
                right,
                ..
            } => {
                let label = format!("Binary: {operator}");
                let expr_id = self.get_node_id_with_class(&key, &label, "expressionNode");
                let left_id = self.visit_expression(left);
                let right_id = self.visit_expression(right);
                self.connect(&expr_id, &left_id, Some("left"));
                self.connect(&expr_id, &right_id, Some("right"));
                expr_id
            }
            Expression::Paren(inner, _) => {
                let label = "Paren".to_string();
                let expr_id = self.get_node_id_with_class(&key, &label, "expressionNode");
                let inner_id = self.visit_expression(inner);
                self.connect(&expr_id, &inner_id, Some("expr"));
                expr_id
            }
            Expression::MethodCall {
                receiver, method, ..
            } => {
                let label = format!("Method: {receiver}.{method}");
                let expr_id = self.get_node_id_with_class(&key, &label, "expressionNode");
                let receiver_id = self.visit_expression(receiver);
                self.connect(&expr_id, &receiver_id, Some("receiver"));
                expr_id
            }
        };

        expr_id
    }

    /// Visit a class constructor
    fn visit_class_constructor(&mut self, constructor: &ClassConstructor) -> String {
        let key = format!("constructor_{constructor:p}");
        let label = format!("ClassConstructor: {}", constructor.class_name.name());
        let constructor_id = self.get_node_id(&key, &label);

        for (idx, field) in constructor.fields.iter().enumerate() {
            let field_id = self.visit_class_constructor_field(field, idx);
            self.connect(&constructor_id, &field_id, Some("field"));
        }

        constructor_id
    }

    /// Visit a class constructor field
    fn visit_class_constructor_field(
        &mut self,
        field: &ClassConstructorField,
        index: usize,
    ) -> String {
        let key = format!("constructor_field_{index}_{field:p}");
        let (label, expr) = match field {
            ClassConstructorField::Named(name, expr) => (format!("Field: {}", name.name()), expr),
            ClassConstructorField::Spread(expr) => ("Spread".to_string(), expr),
        };
        let field_id = self.get_node_id(&key, &label);

        let expr_id = self.visit_expression(expr);
        self.connect(&field_id, &expr_id, Some("value"));

        field_id
    }

    /// Visit an expression block
    fn visit_expression_block(&mut self, block: &ExpressionBlock) -> String {
        let key = format!("expr_block_{block:p}");
        let label = "ExpressionBlock";
        let block_id = self.get_node_id(&key, label);

        // Visit statements
        for (idx, stmt) in block.stmts.iter().enumerate() {
            let stmt_id = self.visit_stmt(stmt, idx);
            self.connect(&block_id, &stmt_id, Some("stmt"));
        }

        // Visit the final expression (if present)
        if let Some(expr) = &block.expr {
            let expr_id = self.visit_expression(expr);
            self.connect(&block_id, &expr_id, Some("expr"));
        }

        // Visit headers that apply to the final expression - attach to block instead of expr
        for (idx, header) in block.expr_headers.iter().enumerate() {
            let header_id = self.visit_header(header, idx);
            self.connect(&block_id, &header_id, Some("annotation"));
        }

        block_id
    }

    /// Visit a header
    fn visit_header(&mut self, header: &Header, index: usize) -> String {
        let key = format!("header_{index}_{header:p}");
        let label = if self.use_styling {
            format!("<b>{}</b><br/><b>Level:</b> {}", header.title, header.level)
        } else {
            format!("{} (Level: {})", header.title, header.level)
        };
        self.get_node_id_with_class(&key, &label, "headerNode")
    }

    /// Visit a statement
    fn visit_stmt(&mut self, stmt: &Stmt, index: usize) -> String {
        let key = format!("stmt_{index}_{stmt:p}");

        match stmt {
            Stmt::Let(let_stmt) => {
                let label = format!("Let: {}", let_stmt.identifier.name());
                let stmt_id = self.get_node_id_with_class(&key, &label, "statementNode");

                // Visit annotations
                for (idx, annotation) in let_stmt.annotations.iter().enumerate() {
                    let annotation_id = self.visit_header(annotation, idx);
                    self.connect(&stmt_id, &annotation_id, Some("annotation"));
                }

                let expr_id = self.visit_expression(&let_stmt.expr);
                self.connect(&stmt_id, &expr_id, Some("value"));
                stmt_id
            }
            Stmt::ForLoop(for_stmt) => {
                let label = format!("For: {}", for_stmt.identifier.name());
                let stmt_id = self.get_node_id_with_class(&key, &label, "statementNode");

                // Visit annotations
                for (idx, annotation) in for_stmt.annotations.iter().enumerate() {
                    let annotation_id = self.visit_header(annotation, idx);
                    self.connect(&stmt_id, &annotation_id, Some("annotation"));
                }

                let iterable_id = self.visit_expression(&for_stmt.iterator);
                let body_id = self.visit_expression_block(&for_stmt.body);
                self.connect(&stmt_id, &iterable_id, Some("iterable"));
                self.connect(&stmt_id, &body_id, Some("body"));
                stmt_id
            }
            Stmt::Expression(es) => {
                let label = "ExprStmt".to_string();
                let stmt_id = self.get_node_id_with_class(&key, &label, "statementNode");
                for (idx, annotation) in es.annotations.iter().enumerate() {
                    let annotation_id = self.visit_header(annotation, idx);
                    self.connect(&stmt_id, &annotation_id, Some("annotation"));
                }
                let expr_id = self.visit_expression(&es.expr);
                self.connect(&stmt_id, &expr_id, Some("expr"));
                stmt_id
            }
            Stmt::Assign(assign_stmt) => {
                let label = format!("Assign: {}", assign_stmt.expr);
                let stmt_id = self.get_node_id_with_class(&key, &label, "statementNode");
                let expr_id = self.visit_expression(&assign_stmt.expr);
                self.connect(&stmt_id, &expr_id, Some("value"));
                stmt_id
            }
            Stmt::AssignOp(assign_op_stmt) => {
                let label = format!(
                    "AssignOp: {} {}",
                    assign_op_stmt.expr, assign_op_stmt.assign_op
                );
                let stmt_id = self.get_node_id_with_class(&key, &label, "statementNode");
                let expr_id = self.visit_expression(&assign_op_stmt.expr);
                self.connect(&stmt_id, &expr_id, Some("value"));
                stmt_id
            }
            Stmt::CForLoop(c_for_stmt) => {
                let label = "C-For Loop".to_string();
                let stmt_id = self.get_node_id_with_class(&key, &label, "statementNode");

                // Visit init statement if present
                if let Some(init_stmt) = &c_for_stmt.init_stmt {
                    let init_id = self.visit_stmt(init_stmt, 0);
                    self.connect(&stmt_id, &init_id, Some("init"));
                }

                // Visit condition if present
                if let Some(condition) = &c_for_stmt.condition {
                    let condition_id = self.visit_expression(condition);
                    self.connect(&stmt_id, &condition_id, Some("condition"));
                }

                // Visit after statement if present
                if let Some(after_stmt) = &c_for_stmt.after_stmt {
                    let after_id = self.visit_stmt(after_stmt, 0);
                    self.connect(&stmt_id, &after_id, Some("after"));
                }

                let body_id = self.visit_expression_block(&c_for_stmt.body);
                self.connect(&stmt_id, &body_id, Some("body"));
                stmt_id
            }
            Stmt::WhileLoop(while_stmt) => {
                let label = "While Loop".to_string();
                let stmt_id = self.get_node_id_with_class(&key, &label, "statementNode");

                let condition_id = self.visit_expression(&while_stmt.condition);
                let body_id = self.visit_expression_block(&while_stmt.body);
                self.connect(&stmt_id, &condition_id, Some("condition"));
                self.connect(&stmt_id, &body_id, Some("body"));
                stmt_id
            }
            Stmt::Semicolon(expr) => {
                let label = "Semicolon Expression".to_string();
                let stmt_id = self.get_node_id_with_class(&key, &label, "statementNode");
                let expr_id = self.visit_expression(&expr.expr);
                self.connect(&stmt_id, &expr_id, Some("expr"));
                stmt_id
            }
            Stmt::Break(_) => {
                let label = "Break".to_string();
                self.get_node_id_with_class(&key, &label, "statementNode")
            }
            Stmt::Continue(_) => {
                let label = "Continue".to_string();
                self.get_node_id_with_class(&key, &label, "statementNode")
            }
            Stmt::Return(return_stmt) => {
                let label = "Return".to_string();
                let stmt_id = self.get_node_id_with_class(&key, &label, "statementNode");
                let value_id = self.visit_expression(&return_stmt.value);
                self.connect(&stmt_id, &value_id, Some("value"));
                stmt_id
            }
            Stmt::Assert(assert_stmt) => {
                let label = "Assert".to_string();
                let stmt_id = self.get_node_id_with_class(&key, &label, "statementNode");
                let value_id = self.visit_expression(&assert_stmt.value);
                self.connect(&stmt_id, &value_id, Some("condition"));
                stmt_id
            }
            Stmt::WatchOptions(watch_opts) => {
                let label = format!("WatchOptions: {}", watch_opts.variable.name());
                self.get_node_id_with_class(&key, &label, "statementNode")
            }
            Stmt::WatchNotify(watch_notify) => {
                let label = format!("WatchNotify: {}", watch_notify.variable.name());
                self.get_node_id_with_class(&key, &label, "statementNode")
            }
        }
    }

    /// Visit an assignment
    fn visit_assignment(&mut self, assignment: &Assignment) -> String {
        let key = format!("assignment_{assignment:p}");
        let label = format!("Assignment: {}", assignment.identifier().name());
        let assignment_id = self.get_node_id(&key, &label);

        let type_id = self.visit_field_type_value(&assignment.value);
        self.connect(&assignment_id, &type_id, Some("value"));

        assignment_id
    }

    /// Visit a field type value
    fn visit_field_type_value(&mut self, field_type: &FieldType) -> String {
        let key = format!("field_type_value_{field_type:p}");
        let label = format!("Type: {}", field_type.name());
        self.get_node_id(&key, &label)
    }

    /// Visit a top-level assignment
    fn visit_top_level_assignment(&mut self, assignment: &TopLevelAssignment) -> String {
        let key = format!("top_assignment_{assignment:p}");
        let label = format!("Assignment: {}", assignment.stmt.identifier.name());
        let assignment_id = self.get_node_id_with_class(&key, &label, "statementNode");

        let expr_id = self.visit_expression(&assignment.stmt.expr);
        self.connect(&assignment_id, &expr_id, Some("value"));

        assignment_id
    }

    /// Visit an expression function
    fn visit_expr_fn(&mut self, expr_fn: &ExprFn) -> String {
        let key = format!("expr_fn_{expr_fn:p}");
        let label = format!("ExprFn: {}", expr_fn.name.name());
        let expr_fn_id = self.get_node_id_with_class(&key, &label, "functionNode");

        // Visit annotations
        for (idx, annotation) in expr_fn.annotations.iter().enumerate() {
            let annotation_id = self.visit_header(annotation, idx);
            self.connect(&expr_fn_id, &annotation_id, Some("annotation"));
        }

        // Visit arguments
        let args_id = self.visit_block_args(&expr_fn.args);
        self.connect(&expr_fn_id, &args_id, Some("args"));

        // Visit return type if present
        if let Some(ret) = &expr_fn.return_type {
            let ret_id = self.visit_field_type_value(ret);
            self.connect(&expr_fn_id, &ret_id, Some("returns"));
        }

        // Visit the body expression block
        let body_id = self.visit_expression_block(&expr_fn.body);
        self.connect(&expr_fn_id, &body_id, Some("body"));

        expr_fn_id
    }

    /// Visit a template string
    fn visit_template_string(&mut self, template: &TemplateString) -> String {
        let key = format!("template_{template:p}");
        let label = format!("TemplateString: {}", template.identifier().name());
        let template_id = self.get_node_id(&key, &label);

        // Visit input parameters if present
        if let Some(input) = &template.input {
            let input_id = self.visit_block_args(input);
            self.connect(&template_id, &input_id, Some("input"));
        }

        // Visit the template value
        let value_id = self.visit_expression(&template.value);
        self.connect(&template_id, &value_id, Some("value"));

        // Visit attributes
        for (idx, attr) in template.attributes.iter().enumerate() {
            let attr_id = self.visit_attribute(attr, idx);
            self.connect(&template_id, &attr_id, Some("attr"));
        }

        template_id
    }

    /// Render the header-only diagram using simplified HeaderIndex
    fn render_headers_only(&mut self, ast: &Ast) {
        let index: HeaderIndex = HeaderCollector::collect(ast, None);

        // Create all header nodes first
        let mut header_node_ids: HashMap<String, String> = HashMap::new();
        for header in &index.headers {
            let node_id = self.get_node_id_with_class(
                &header.id,
                &format!(
                    "<b>{}</b><br/><b>Level:</b> {}<br/><b>Span:</b> {}:{}-{}",
                    header.title,
                    header.level,
                    header.span.file.path(),
                    header.span.start,
                    header.span.end
                ),
                "headerNode",
            );
            header_node_ids.insert(header.id.clone(), node_id);
        }

        // Build set of headers that have at least one incoming nested edge (via Hid)
        let mut has_incoming_nested: std::collections::HashSet<&str> =
            std::collections::HashSet::new();
        for (_from_hid, to_hid) in index.nested_edges_hid_iter() {
            if let Some(to_hdr) = index.get_by_hid(*to_hid) {
                has_incoming_nested.insert(to_hdr.id.as_str());
            }
        }

        // Connect markdown edges only for headers that do NOT have incoming nested edges
        for header in &index.headers {
            if let Some(parent) = &header.parent_id {
                if has_incoming_nested.contains(header.id.as_str()) {
                    continue;
                }
                if let (Some(parent_node), Some(this_node)) =
                    (header_node_ids.get(parent), header_node_ids.get(&header.id))
                {
                    self.connect(parent_node, this_node, Some("markdown"));
                }
            }
        }

        // Render nested edges from Hid pairs
        for (from_hid, to_hid) in index.nested_edges_hid_iter() {
            if let (Some(from_hdr), Some(to_hdr)) =
                (index.get_by_hid(*from_hid), index.get_by_hid(*to_hid))
            {
                if let (Some(from_node), Some(to_node)) = (
                    header_node_ids.get(&from_hdr.id),
                    header_node_ids.get(&to_hdr.id),
                ) {
                    self.connect(from_node, to_node, Some("nested"));
                }
            }
        }

        // Note: scope roots can be obtained from the first header per scope via by_scope
    }
}

impl Default for MermaidDiagramGenerator {
    fn default() -> Self {
        Self::new()
    }
}
