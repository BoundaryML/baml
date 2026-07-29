function _tool_schemas(tools: root.ai.tools.Tool[]) -> json[] throws never {
    tools.map((tool: root.ai.tools.Tool) -> json {
        {
            "type": "function",
            "function": {
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.input_schema,
                "strict": false,
            },
        }
    })
}

implements root.ai.tools.ToolCallingProvider for Chat {
  function begin<T>(self, task: root.ai.Task<T>) -> root.ai.Conversation throws never {
    //# Start the native OpenAI conversation
    let history = task.messages().to_history();
    Conversation {
      owner: self,
      history: history,
      tools_json: _tool_schemas(task.tools),
      wire_messages: task.prompt.messages().map((message: baml.llm.PromptMessage) -> json {
        {
          "role": root.ai.messages._prompt_role_or_user(message.role),
          "content": message.content,
        }
      }),
    }
  }

  function step<T>(
    self,
    conversation: root.ai.Conversation,
    tools: root.ai.tools.Tool[],
  ) -> root.ai.tools.ModelStep<T> throws root.ai.Failure | baml.errors.UnknownError {
    //# Resume the native OpenAI conversation
    let state = match (conversation) {
      let value: Conversation => value,
      _ => throw root.ai.Failure {
        message: "OpenAI received another provider's conversation",
      },
    };
    //# Build the request from the active tool roster
    state.tools_json = _tool_schemas(tools);
    let key = match (self.api_key) {
      let value: string => value,
      null => throw baml.errors.UnknownError.with_message<never>(null, "OpenAI tool calling requires api_key"),
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
      _ => throw baml.errors.UnknownError.with_message<never>(e, "openai tools send failed"),
    };
    let text = response.text() catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(e, "openai tools body failed"),
    };
    log.info(baml.json.stringify({
      "event": "llm.tools.response",
      "provider": "openai",
      "model": self.model,
      "status_code": response.status_code,
      "response_bytes": text.length(),
    }));
    //# Parse the provider response and usage
    let wire: ToolResponse = baml.sap.parse<ToolResponse>(text) catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(e, "openai tools parse failed"),
    };
    let choice = match (wire.choices.at(0)) {
      let value: ToolChoice => value,
      null => throw baml.errors.UnknownError.with_message<never>(text, "openai tools returned no choice"),
    };
    let message = choice.message;
    let meta = root.ai.ResponseMetadata {
      provider: self.name(),
      model: wire.model ?? self.model,
      request_id: wire.id,
      finish_reason: choice.finish_reason,
      usage: match (wire.usage) {
        let value: ToolUsage => root.ai.Usage {
          input_tokens: value.prompt_tokens,
          output_tokens: value.completion_tokens,
          cached_input_tokens: baml.json.path_or<int>(
            value.prompt_tokens_details,
            ".cached_tokens",
            0,
          ),
          reasoning_tokens: baml.json.path_or<int>(
            value.completion_tokens_details,
            ".reasoning_tokens",
            0,
          ),
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
        //## Preserve native tool calls in the conversation
        state.wire_messages.push({ "role": "assistant", "tool_calls": raw_calls });
        state.history.entries.push(root.ai.ChatMessage {
          message_role: root.ai.MessageRole.Assistant,
          content: [root.ai.messages.ProviderDataPart {
            provider: "openai",
            data: { "tool_calls": raw_calls },
          }],
          metadata: null,
        });
        let calls = root.ai.tools.ToolCalls {
          calls: raw_calls.map((raw: json) -> root.ai.tools.ToolCall {
            let args_text = baml.json.path_or(raw, ".function.arguments", "{}");
            root.ai.tools.ToolCall {
              id: baml.json.path_or(raw, ".id", ""),
              name: baml.json.path_or(raw, ".function.name", ""),
              args: baml.json.parse(args_text) catch (e) { _ => null },
            }
          }),
        };
        root.ai.tools.ModelStep<T> { outcome: calls, metadata: meta }
      },
      null => {
        //## Parse the final assistant value
        state.history.entries.push(root.ai.ChatMessage.assistant(message.content ?? ""));
        let value = baml.sap.parse<T>(message.content ?? "") catch (e) {
          _ => throw baml.errors.UnknownError.with_message<never>(e, "openai agent final parse failed"),
        };
        root.ai.tools.ModelStep<T> { outcome: value, metadata: meta }
      },
    }
  }

  function submit(
    self,
    conversation: root.ai.Conversation,
    results: root.ai.tools.ToolResult[],
  ) -> root.ai.Conversation throws root.ai.Failure {
    //# Resume the conversation receiving tool results
    let state = match (conversation) {
      let value: Conversation => value,
      _ => throw root.ai.Failure {
        message: "OpenAI received another provider's conversation",
      },
    };
    //# Append every tool result to provider wire state
    for (let result in results) {
      state.wire_messages.push({
        "role": "tool",
        "tool_call_id": result.id,
        "content": baml.json.stringify(result.output),
      });
      state.history.entries.push(root.ai.ChatMessage {
        message_role: root.ai.MessageRole.Tool,
        content: [root.ai.messages.ProviderDataPart {
          provider: "openai",
          data: {
            "tool_call_id": result.id,
            "output": result.output,
          },
        }],
        metadata: null,
      });
    }
    state
  }
}
