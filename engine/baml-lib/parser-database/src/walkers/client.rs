use internal_baml_schema_ast::ast::Expression;

use crate::{
    ast::{self, WithIdentifier},
    types::ClientProperties,
};

/// A `function` declaration in the Prisma schema.
pub type ClientWalker<'db> = super::Walker<'db, ast::ClientId>;

impl<'db> ClientWalker<'db> {
    /// The ID of the function in the db
    pub fn client_id(self) -> ast::ClientId {
        self.id
    }

    /// The AST node.
    pub fn ast_client(self) -> &'db ast::Client {
        &self.db.ast[self.id]
    }

    /// The properties of the variant.
    pub fn properties(self) -> &'db ClientProperties {
        &self.db.types.client_properties[&self.id]
    }

    /// The provider for the client, e.g. baml-openai-chat
    pub fn provider(self) -> &'db str {
        self.properties().provider.0.as_str()
    }

    /// The model specified for the client, e.g. "gpt-3.5-turbo"
    pub fn model(self) -> Option<&'db str> {
        let Some((_, model)) = self.properties().options.iter().find(|(k, _)| k == "model") else {
            return None;
        };
        let Some((model, _)) = model.as_string_value() else {
            return None;
        };
        Some(model)
    }

    /// Returns the list of all non-strategy clients (i.e. flattens fallback/round-robin clients to their constituent clients)
    pub fn flat_clients(self) -> Vec<ClientWalker<'db>> {
        // TODO(sam): how are fallback/round-robin clients represented here?
        let provider = self.properties().provider.0.as_str();

        if provider == "baml-fallback" || provider == "baml-round-robin" {
            let Some((_, strategy)) = self
                .properties()
                .options
                .iter()
                .find(|(k, _)| k == "strategy")
            else {
                return vec![];
            };
            let Expression::Array(strategy, _span) = strategy else {
                return vec![];
            };

            let mut clients = vec![];
            for entry in strategy {
                if let Some((s, _)) = entry.as_string_value() {
                    clients.push(s);
                }
                if let Some((m, _)) = entry.as_map() {
                    if let Some((_, client_name)) = m
                        .iter()
                        .filter(|(k, _)| k.as_string_value().map_or(false, |(s, _)| s == "client"))
                        .nth(0)
                    {
                        if let Some((client_name, _)) = client_name.as_string_value() {
                            clients.push(client_name);
                        };
                    };
                }
            }
            let clients = clients
                .into_iter()
                .filter_map(|client_name| self.db.find_client(client_name))
                .flat_map(|client| client.flat_clients().into_iter())
                .collect::<Vec<_>>();

            return clients;
        }

        vec![self]
    }
}

// with identifier
impl<'db> WithIdentifier for ClientWalker<'db> {
    fn identifier(&self) -> &ast::Identifier {
        self.ast_client().identifier()
    }
}
