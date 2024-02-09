use internal_baml_prompt_parser::ast::ChatBlock;
use internal_baml_schema_ast::ast::{Identifier, WithName};

use crate::{
    ast::{self, WithIdentifier},
    types::{ToStringAttributes, VariantProperties},
};

use super::{ClassWalker, ClientWalker, EnumWalker, FunctionWalker, Walker};

/// A `function` declaration in the Prisma schema.
pub type VariantWalker<'db> = Walker<'db, ast::VariantConfigId>;

/// The representation of a prompt.
pub enum PromptRepr<'a> {
    /// For single string prompts
    /// Prompt + Any used input replacers
    String(String, Vec<String>),

    /// For prompts with multiple parts
    /// ChatBlock + Prompt + Any used input replacers
    Chat(Vec<(Option<&'a ChatBlock>, String)>, Vec<String>),
}

impl<'db> VariantWalker<'db> {
    /// The name of the function.
    pub fn function_identifier(self) -> &'db Identifier {
        self.ast_variant().function_name()
    }

    /// The ID of the function in the db
    pub fn variant_id(self) -> ast::VariantConfigId {
        self.id
    }

    /// Helper to access every client.
    pub fn client(self) -> Option<ClientWalker<'db>> {
        self.db
            .types
            .variant_properties
            .get(&self.id)
            .and_then(|f| self.db.find_client(&f.client.value))
    }

    /// The function node.
    pub fn walk_function(self) -> Option<FunctionWalker<'db>> {
        self.db.find_function(self.function_identifier())
    }

    /// The AST node.
    pub fn ast_variant(self) -> &'db ast::Variant {
        &self.db.ast[self.id]
    }

    /// Finds a serializer by name
    pub fn find_serializer_attributes(self, name: &str) -> Option<&'db ToStringAttributes> {
        self.ast_variant()
            .iter_serializers()
            .find(|(_, s)| s.name() == name)
            .and_then(|(idx, _s)| {
                self.db.types.variant_attributes[&self.id]
                    .serializers
                    .get(&idx)
                    .and_then(|f| f.serilizer.as_ref())
            })
    }

    /// Finds a serializer for a field by name
    pub fn find_serializer_field_attributes(
        self,
        name: &str,
        field_name: &str,
    ) -> Option<&'db ToStringAttributes> {
        self.ast_variant()
            .iter_serializers()
            .find(|(_, s)| s.name() == name)
            .and_then(|(idx, s)| {
                let fid = s.field_id_for(field_name);

                if let Some(fid) = fid {
                    self.db.types.variant_attributes[&self.id]
                        .serializers
                        .get(&idx)
                        .and_then(|s| s.field_serilizers.get(&fid))
                } else {
                    None
                }
            })
    }

    /// The properties of the variant.
    pub fn properties(self) -> &'db VariantProperties {
        &self.db.types.variant_properties[&self.id]
    }

    /// The prompt representation.
    pub fn to_prompt<'a>(&'a self) -> PromptRepr<'a> {
        // If the prompt is a chat prompt, we need to collect the chat blocks
        // by splitting the prompt into parts.
        let properties = self.properties();

        let (input, output, chats) = &properties.replacers;

        // Replace all the inputs with the input replacers
        let mut used_inputs = vec![];
        let prompt = input
            .iter()
            .fold(properties.prompt.value.clone(), |prompt, (k, val)| {
                // Only add the input if it's used in the prompt
                if prompt.contains(&k.key()) {
                    used_inputs.push(val.clone());
                    prompt.replace(&k.key(), &format!("{{{}}}", val))
                } else {
                    prompt
                }
            });
        // Replace all the outputs with the output replacers
        let prompt = output.iter().fold(prompt, |prompt, (k, val)| {
            prompt.replace(&k.key(), &format!("{}", val))
        });

        used_inputs.sort();

        if chats.is_empty() {
            PromptRepr::String(prompt, used_inputs)
        } else {
            // Split the prompt into parts based on the chat blocks.
            let mut last_idx = 0;
            let mut parts = vec![];
            for chat in chats {
                let splitter = chat.key();
                let idx = prompt[last_idx..].find(&splitter);
                if let Some(idx) = idx {
                    last_idx += idx + splitter.len();
                    parts.push((chat, (idx, last_idx)));
                }
            }

            // Each chat block owns a part of the prompt. until the next chat block.
            PromptRepr::Chat(
                parts
                    .iter()
                    .enumerate()
                    .map(|(idx, &(chat, (_, start)))| {
                        let end = if idx + 1 < parts.len() {
                            parts[idx + 1].1 .0
                        } else {
                            prompt.len()
                        };

                        (Some(chat), prompt[start..end].to_string())
                    })
                    .collect(),
                used_inputs,
            )
        }
    }

    /// Get the output of a function.
    pub fn output_type(self) -> impl ExactSizeIterator<Item = super::ArgWalker<'db>> {
        self.walk_function().unwrap().walk_output_args()
    }

    /// The name of the function.
    pub fn output_required_enums(self) -> impl Iterator<Item = EnumWalker<'db>> {
        if let Some((idx, _)) = self.properties().output_adapter {
            let adapter = &self.ast_variant()[idx];

            return adapter
                .from
                .flat_idns()
                .iter()
                .filter_map(|f| self.db.find_enum(f))
                .collect::<Vec<_>>()
                .into_iter();
        }

        self.walk_function()
            .unwrap()
            .walk_output_args()
            .map(|f| f.required_enums())
            .flatten()
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// The name of the function.
    pub fn output_required_classes(self) -> impl Iterator<Item = ClassWalker<'db>> {
        if let Some((idx, _)) = self.properties().output_adapter {
            let adapter = &self.ast_variant()[idx];

            return adapter
                .from
                .flat_idns()
                .iter()
                .filter_map(|f| self.db.find_class(f))
                .collect::<Vec<_>>()
                .into_iter();
        }

        self.walk_function()
            .unwrap()
            .walk_output_args()
            .map(|f| f.required_classes())
            .flatten()
            .collect::<Vec<_>>()
            .into_iter()
    }
}

impl<'db> WithIdentifier for VariantWalker<'db> {
    fn identifier(&self) -> &'db Identifier {
        self.ast_variant().identifier()
    }
}
