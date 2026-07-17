class OpenAiToolMessage {
  content: string?,
  tool_calls: json[]?,
}

class OpenAiToolChoice {
  message: OpenAiToolMessage,
  finish_reason: string?,
}

class OpenAiToolUsage {
  prompt_tokens: int,
  completion_tokens: int,
  prompt_tokens_details: json?,
  completion_tokens_details: json?,
}

class OpenAiToolResponse {
  id: string?,
  model: string?,
  choices: OpenAiToolChoice[],
  usage: OpenAiToolUsage?,
}

class OpenAiToolTranscript {
  owner: OpenAi,
  history: Conversation,
  tools_json: json[],
  wire_messages: json[],

  implements Transcript {
    function provider(self) -> Provider throws never { self.owner }
    function messages(self) -> Messages throws never { self.history }
    function conversation(self) -> Conversation throws never { self.history }
  }
}

function openai_tool_schemas(tools: Tool[]) -> json[] throws baml.errors.UnknownError {
  tools.map((tool: Tool) -> json {
    let schema = if (tool.input_schema != null) {
      tool.input_schema
    } else {
      baml.schema.json_schema(tool.parameters) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["tool schema generation failed"] },
      }
    };
    {
      "type": "function",
      "function": {
        "name": tool.name,
        "description": tool.description,
        "parameters": schema,
        "strict": false,
      },
    }
  })
}

implements ToolCallingProvider for OpenAi {
  function begin<T>(self, task: Task<T>) -> Transcript
      throws baml.errors.UnknownError {
    //# Start the native OpenAI tool transcript
    let history = task.messages().to_conversation();
    OpenAiToolTranscript {
      owner: self,
      history: history,
      tools_json: openai_tool_schemas(task.tools),
      wire_messages: [{
        "role": "user",
        "content": baml.llm.prompt_to_text(task.prompt),
      }],
    }
  }

  function step<T>(self, transcript: Transcript, tools: Tool[]) -> ModelStep<T>
      throws baml.errors.ToolError | baml.errors.UnknownError {
    //# Resume the native OpenAI tool transcript
    let state = match (transcript) {
      let value: OpenAiToolTranscript => value,
      _ => throw baml.errors.ToolError { message: "OpenAI received another provider's transcript" },
    };
    //# Build the request from the active tool roster
    state.tools_json = openai_tool_schemas(tools);
    let key = match (self.api_key) {
      let value: string => value,
      null => throw baml.errors.UnknownError {
        data: null,
        message: ["OpenAI tool calling requires api_key"],
      },
    };
    let body: map<string, json> = {
      "model": self.model,
      "messages": state.wire_messages,
      "tools": state.tools_json,
      "tool_choice": "auto",
      "parallel_tool_calls": true,
      "reasoning_effort": "none",
    };
    log.info(baml.json.stringify({
      "event": "llm.tools.request",
      "provider": "openai",
      "model": self.model,
      "message_count": state.wire_messages.length(),
      "tool_count": state.tools_json.length(),
    }));
    //# Call OpenAI with native tool schemas
    let response = baml.http.send(baml.http.Request {
      method: "POST",
      url: (self.base_url ?? "https://api.openai.com/v1") + "/chat/completions",
      headers: {
        "Authorization": "Bearer " + key,
        "Content-Type": "application/json",
      },
      body: baml.json.stringify(body),
    }) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["openai tools send failed"] },
    };
    let text = response.text() catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["openai tools body failed"] },
    };
    log.info(baml.json.stringify({
      "event": "llm.tools.response",
      "provider": "openai",
      "model": self.model,
      "status_code": response.status_code,
      "response_bytes": text.length(),
    }));
    //# Parse the provider response and usage
    let wire: OpenAiToolResponse = baml.sap.parse<OpenAiToolResponse>(text) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["openai tools parse failed"] },
    };
    let choice = match (wire.choices.at(0)) {
      let value: OpenAiToolChoice => value,
      null => throw baml.errors.UnknownError { data: text, message: ["openai tools returned no choice"] },
    };
    let message = choice.message;
    let meta = Meta {
      provider: self.name(),
      model: wire.model ?? self.model,
      request_id: wire.id,
      finish_reason: choice.finish_reason,
      usage: match (wire.usage) {
        let value: OpenAiToolUsage => Usage {
          input_tokens: value.prompt_tokens,
          output_tokens: value.completion_tokens,
          cached_input_tokens: baml.json.path_or<int>(value.prompt_tokens_details, ".cached_tokens", 0),
          reasoning_tokens: baml.json.path_or<int>(value.completion_tokens_details, ".reasoning_tokens", 0),
          cost_usd: null,
        },
        null => null,
      },
      attributes: {},
      raw: text,
    };
    //# Return tool calls or the typed final value
    match (message.tool_calls) {
      let raw_calls: json[] => {
        //## Preserve native tool calls in the transcript
        state.wire_messages.push({ "role": "assistant", "tool_calls": raw_calls });
        state.history.entries.push(ChatMessage {
          message_role: MessageRole.Assistant,
          content: [ProviderDataPart { provider: "openai", data: { "tool_calls": raw_calls } }],
          metadata: null,
        });
        let calls = ToolCalls {
          calls: raw_calls.map((raw: json) -> ToolCall {
            let args_text = baml.json.path_or(raw, ".function.arguments", "{}");
            ToolCall {
              id: baml.json.path_or(raw, ".id", ""),
              name: baml.json.path_or(raw, ".function.name", ""),
              args: baml.json.parse(args_text) catch (e) { _ => null },
            }
          }),
        };
        ModelStep<T> { outcome: calls, meta: meta }
      },
      null => {
        //## Parse the final assistant value
        state.history.entries.push(ChatMessage.assistant(message.content ?? ""));
        let value = baml.sap.parse<T>(message.content ?? "") catch (e) {
          _ => throw baml.errors.UnknownError { data: e, message: ["openai agent final parse failed"] },
        };
        ModelStep<T> { outcome: value, meta: meta }
      },
    }
  }

  function submit(self, transcript: Transcript, results: ToolResult[]) -> Transcript
      throws baml.errors.ToolError {
    //# Resume the transcript receiving tool results
    let state = match (transcript) {
      let value: OpenAiToolTranscript => value,
      _ => throw baml.errors.ToolError { message: "OpenAI received another provider's transcript" },
    };
    //# Append every tool result to provider wire state
    for (let result in results) {
      state.wire_messages.push({
        "role": "tool",
        "tool_call_id": result.id,
        "content": baml.json.stringify(result.output),
      });
      state.history.entries.push(ChatMessage {
        message_role: MessageRole.Tool,
        content: [ProviderDataPart {
          provider: "openai",
          data: { "tool_call_id": result.id, "output": result.output },
        }],
        metadata: null,
      });
    }
    state
  }
}

function message_role_name(role: MessageRole) -> string throws never {
  match (role) {
    MessageRole.System => "system",
    MessageRole.User => "user",
    MessageRole.Assistant => "assistant",
    MessageRole.Tool => "tool",
  }
}

function message_text(message: Message) -> string throws never {
  message.parts().map((part: MessagePart) -> string { part.text() ?? "" }).join("")
}

implements TranscriptImportProvider for OpenAi {
  function import_conversation(self, conversation: Conversation) -> TranscriptImport throws never {
    let wire: json[] = conversation.entries.map((message: Message) -> json {
      {
        "role": message_role_name(message.role()),
        "content": message_text(message),
      }
    });
    TranscriptImport {
      transcript: OpenAiToolTranscript {
        owner: self,
        history: conversation,
        tools_json: [],
        wire_messages: wire,
      },
      fidelity: TranscriptFidelity.MessagesOnly,
      warnings: ["provider-specific continuation metadata was not imported"],
    }
  }
}

implements ResumableToolCallingProvider for OpenAi {
  function save_transcript(self, transcript: Transcript) -> TranscriptToken
      throws baml.errors.UnknownError {
    let state = match (transcript) {
      let value: OpenAiToolTranscript => value,
      _ => throw baml.errors.UnknownError { data: null, message: ["wrong OpenAI transcript"] },
    };
    TranscriptToken {
      provider: self.name(),
      version: 1,
      sealed: baml.json.stringify({
        "messages": state.wire_messages,
        "tools": state.tools_json,
      }),
    }
  }

  function restore_transcript(self, token: TranscriptToken) -> Transcript
      throws baml.errors.UnknownError {
    if (token.provider != self.name() || token.version != 1) {
      throw baml.errors.UnknownError {
        data: token,
        message: ["OpenAI transcript token has the wrong provider or version"],
      };
    }
    let state = baml.json.parse(token.sealed) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["invalid OpenAI transcript token"] },
    };
    OpenAiToolTranscript {
      owner: self,
      history: Conversation.empty(),
      tools_json: baml.json.path_or<json[]>(state, ".tools", []),
      wire_messages: baml.json.path_or<json[]>(state, ".messages", []),
    }
  }
}
