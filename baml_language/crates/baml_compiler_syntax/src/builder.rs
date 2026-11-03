//! Test utilities for building syntax trees programmatically.

use rowan::{GreenNode, GreenNodeBuilder};

use crate::SyntaxKind;

/// Builder for constructing syntax trees.
pub(crate) struct SyntaxTreeBuilder {
    builder: GreenNodeBuilder<'static>,
}

impl SyntaxTreeBuilder {
    /// Create a new tree builder.
    pub(crate) fn new() -> Self {
        Self {
            builder: GreenNodeBuilder::new(),
        }
    }

    /// Start a new node of the given kind.
    pub(crate) fn start_node(&mut self, kind: SyntaxKind) {
        self.builder.start_node(kind.into());
    }

    /// Finish the current node.
    pub(crate) fn finish_node(&mut self) {
        self.builder.finish_node();
    }

    /// Add a token to the tree.
    pub(crate) fn token(&mut self, kind: SyntaxKind, text: &str) {
        self.builder.token(kind.into(), text);
    }

    /// Add whitespace.
    pub(crate) fn ws(&mut self, text: &str) {
        self.token(SyntaxKind::WHITESPACE, text);
    }

    /// Add a newline.
    pub(crate) fn nl(&mut self) {
        self.token(SyntaxKind::NEWLINE, "\n");
    }

    /// Build and consume the builder, returning the green tree.
    pub(crate) fn finish(self) -> GreenNode {
        self.builder.finish()
    }

    /// Build a simple function for testing.
    pub(crate) fn build_function(name: &str, params: &[(&str, &str)], ret_type: &str) -> GreenNode {
        let mut builder = Self::new();

        builder.start_node(SyntaxKind::SOURCE_FILE);
        builder.start_node(SyntaxKind::FUNCTION_DEF);

        // function keyword
        builder.token(SyntaxKind::KW_FUNCTION, "function");
        builder.ws(" ");

        // function name
        builder.token(SyntaxKind::WORD, name);

        // parameters
        builder.start_node(SyntaxKind::PARAMETER_LIST);
        builder.token(SyntaxKind::L_PAREN, "(");

        for (i, (param_name, param_type)) in params.iter().enumerate() {
            if i > 0 {
                builder.token(SyntaxKind::COMMA, ",");
                builder.ws(" ");
            }

            builder.start_node(SyntaxKind::PARAMETER);
            builder.token(SyntaxKind::WORD, param_name);
            builder.token(SyntaxKind::COLON, ":");
            builder.ws(" ");
            builder.start_node(SyntaxKind::TYPE_EXPR);
            builder.token(SyntaxKind::WORD, param_type);
            builder.finish_node(); // TYPE_EXPR
            builder.finish_node(); // PARAMETER
        }

        builder.token(SyntaxKind::R_PAREN, ")");
        builder.finish_node(); // PARAMETER_LIST

        // return type
        builder.ws(" ");
        builder.token(SyntaxKind::ARROW, "->");
        builder.ws(" ");
        builder.start_node(SyntaxKind::FUNCTION_TYPE_RETURN_TYPE);
        builder.start_node(SyntaxKind::TYPE_EXPR);
        builder.token(SyntaxKind::WORD, ret_type);
        builder.finish_node(); // TYPE_EXPR
        builder.finish_node(); // FUNCTION_TYPE_RETURN_TYPE

        // body
        builder.ws(" ");
        builder.start_node(SyntaxKind::FUNCTION_BODY);
        builder.token(SyntaxKind::L_BRACE, "{");
        builder.nl();
        builder.ws("  ");
        builder.token(SyntaxKind::WORD, "client");
        builder.ws(" ");
        builder.token(SyntaxKind::WORD, "GPT4");
        builder.nl();
        builder.token(SyntaxKind::R_BRACE, "}");
        builder.finish_node(); // FUNCTION_BODY

        builder.finish_node(); // FUNCTION_DEF
        builder.finish_node(); // SOURCE_FILE

        builder.finish()
    }

    /// Build a simple class for testing.
    pub(crate) fn build_class(name: &str, fields: &[(&str, &str)]) -> GreenNode {
        let mut builder = Self::new();

        builder.start_node(SyntaxKind::SOURCE_FILE);
        builder.start_node(SyntaxKind::CLASS_DEF);

        // class keyword
        builder.token(SyntaxKind::KW_CLASS, "class");
        builder.ws(" ");

        // class name
        builder.token(SyntaxKind::WORD, name);
        builder.ws(" ");

        // body
        builder.token(SyntaxKind::L_BRACE, "{");
        builder.nl();

        // fields
        for (field_name, field_type) in fields {
            builder.ws("  ");
            builder.start_node(SyntaxKind::FIELD);
            builder.token(SyntaxKind::WORD, field_name);
            builder.ws(" ");
            builder.start_node(SyntaxKind::TYPE_EXPR);
            builder.token(SyntaxKind::WORD, field_type);
            builder.finish_node(); // TYPE_EXPR
            builder.finish_node(); // FIELD
            builder.nl();
        }

        builder.token(SyntaxKind::R_BRACE, "}");
        builder.finish_node(); // CLASS_DEF
        builder.finish_node(); // SOURCE_FILE

        builder.finish()
    }
}

impl Default for SyntaxTreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}
