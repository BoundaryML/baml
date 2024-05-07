use std::collections::HashMap;

use anyhow::Result;
use internal_baml_jinja::{
  ChatMessagePart, RenderContext, RenderContext_Client, RenderedChatMessage, RenderedPrompt,
  TemplateStringMacro,
};
use napi_derive::napi;

#[napi]
pub struct NapiChatMessagePart {
  internal: ChatMessagePart,
}

#[napi]
impl NapiChatMessagePart {
  #[napi]
  pub fn is_text(&self) -> bool {
    matches!(self.internal, ChatMessagePart::Text(_))
  }

  #[napi]
  pub fn is_image(&self) -> bool {
    matches!(self.internal, ChatMessagePart::Image(_))
  }

  #[napi]
  pub fn text(&self) -> Result<&String> {
    if let ChatMessagePart::Text(t) = &self.internal {
      Ok(t)
    } else {
      anyhow::bail!("Not a text part")
    }
  }
}

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
  pub fn parts(&self) -> Result<Vec<NapiChatMessagePart>> {
    Ok(
      self
        .internal
        .parts
        .iter()
        .map(|p| NapiChatMessagePart {
          internal: p.clone(),
        })
        .collect(),
    )
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
  output_format: String,
}

#[napi]
impl NapiRenderer {
  #[napi(constructor)]
  pub fn new(template: String, output_format: String) -> Result<Self> {
    Ok(NapiRenderer {
      template_string_macros: vec![],
      template,
      output_format,
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
    _args: HashMap<String, serde_json::Value>,
    client: &NapiClient,
    env: HashMap<String, String>,
  ) -> Result<NapiRenderedPrompt> {
    let _ctx = RenderContext {
      client: client.internal.clone(),
      output_format: self.output_format.clone(),
      env,
    };

    Err(anyhow::anyhow!("Not implemented"))

    // let rendered =
    //   RenderedPrompt {
    //     role: "user".to_string(),
    //     prompt: self.template.clone(),
    //   }
    // match rendered {
    //   Ok(r) => Ok(NapiRenderedPrompt { internal: r }),
    //   Err(e) => anyhow::bail!("Failed to render prompt: {}", e),
    // }
  }
}
