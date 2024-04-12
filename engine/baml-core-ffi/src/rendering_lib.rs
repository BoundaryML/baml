use std::collections::HashMap;

use anyhow::Result;
use internal_baml_jinja::{
  RenderContext, RenderContext_Client, RenderedChatMessage, RenderedPrompt, TemplateStringMacro,
};
use napi_derive::napi;

#[napi]
pub struct NapiChatMessage {
  internal: RenderedChatMessage,
}

#[napi]
impl NapiChatMessage {
  #[napi]
  pub fn role(&self) -> &String {
    &self.internal.role
  }

  #[napi]
  pub fn message(&self) -> &String {
    &self.internal.message
  }
}

#[napi]
pub struct NapiRenderedPrompt {
  internal: RenderedPrompt,
}

#[napi]
impl NapiRenderedPrompt {
  #[napi]
  pub fn is_completion(&self) -> bool {
    matches!(self.internal, RenderedPrompt::Completion(_))
  }

  #[napi]
  pub fn is_chat(&self) -> bool {
    matches!(self.internal, RenderedPrompt::Chat(_))
  }

  #[napi]
  pub fn chat_messages(&self) -> Result<Vec<NapiChatMessage>> {
    if let RenderedPrompt::Chat(messages) = &self.internal {
      Ok(
        messages
          .iter()
          .map(|m| NapiChatMessage {
            internal: m.clone(),
          })
          .collect(),
      )
    } else {
      anyhow::bail!("Not a chat prompt")
    }
  }

  #[napi]
  pub fn completion(&self) -> Result<&String> {
    if let RenderedPrompt::Completion(c) = &self.internal {
      Ok(c)
    } else {
      anyhow::bail!("Not a completion prompt")
    }
  }
}

#[napi]
pub struct NapiClient {
  internal: RenderContext_Client,
}

#[napi]
impl NapiClient {
  #[napi(constructor)]
  pub fn new(name: String, provider: String) -> Result<Self> {
    Ok(NapiClient {
      internal: RenderContext_Client { name, provider },
    })
  }
}

#[napi]
pub struct NapiRenderer {
  template_string_macros: Vec<TemplateStringMacro>,
  template: String,
  output_schema: String,
}

#[napi]
impl NapiRenderer {
  #[napi(constructor)]
  pub fn new(template: String, output_schema: String) -> Result<Self> {
    Ok(NapiRenderer {
      template_string_macros: vec![],
      template,
      output_schema,
    })
  }

  #[napi]
  pub fn add_template_string(
    &mut self,
    name: String,
    arg_names: Vec<String>,
    arg_types: Vec<String>,
    template: String,
  ) {
    self.template_string_macros.push(TemplateStringMacro {
      name,
      args: arg_names
        .iter()
        .zip(arg_types.iter())
        .map(|(n, t)| (n.clone(), t.clone()))
        .collect(),
      template,
    });
  }

  #[napi]
  /// Render the prompt with the given arguments
  /// These elements are all dynamic and must be passed in by the caller
  pub fn render(
    &self,
    args: HashMap<String, serde_json::Value>,
    client: &NapiClient,
    env: HashMap<String, String>,
  ) -> Result<NapiRenderedPrompt> {
    let ctx = RenderContext {
      client: client.internal.clone(),
      output_schema: self.output_schema.clone(),
      env,
    };
    let rendered =
      internal_baml_jinja::render_prompt(&self.template, &args, &ctx, &self.template_string_macros);

    match rendered {
      Ok(r) => Ok(NapiRenderedPrompt { internal: r }),
      Err(e) => anyhow::bail!("Failed to render prompt: {}", e),
    }
  }
}
