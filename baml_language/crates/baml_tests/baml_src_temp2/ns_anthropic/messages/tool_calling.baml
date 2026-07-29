implements root.ai.AgentProvider for AnthropicProvider {
  function begin<T>(
    self,
    task: root.ai.Task<T>,
  ) -> root.ai.Conversation
      throws root.ai.Failure | baml.errors.UnknownError {
    root.anthropic.internal._validate_output_config(self);
    if (self.tool_mode == root.anthropic.ToolMode.Prompt) {
      // Prompt mode uses Anthropic's private T | ToolCalls text protocol.
      return root.anthropic.internal._prompt_begin<T>(self, task);
    }
    //# Render the first turn into provider-native Messages blocks
    let request = root.anthropic.internal._messages_built_request(
      self,
      task,
      log_shape = false,
    );
    let request_body = baml.json.parse(request.body) catch (e) {
      _ => {
        throw baml.errors.UnknownError.with_message<never>(
          e,
          "Anthropic initial request body was not JSON",
        )
      },
    };
    root.anthropic.internal.Conversation {
      owner: self,
      history: task.messages().to_history(),
      declared_output_type: root.ai.internal._output_type_fingerprint<T>(),
      system: baml.json.path_or<json[]>(request_body, ".system", []),
      messages: baml.json.path_or<json[]>(request_body, ".messages", []),
      request_url: request.url,
      headers: root.anthropic.internal._headers_without_auth(request.headers),
      active_tools: [],
      last_assistant_content: [],
    }
  }

  function step<T>(
    self,
    conversation: root.ai.Conversation,
    tools: root.ai.tools.Tool[],
  ) -> root.ai.ModelStep<T>
      throws root.ai.Failure | baml.errors.UnknownError {
    root.anthropic.internal._validate_output_config(self);
    root.ai.internal._require_conversation_output_type<T>(
      conversation,
      self.name(),
    );
    if (self.tool_mode == root.anthropic.ToolMode.Prompt) {
      return root.anthropic.internal._prompt_step<T>(self, conversation, tools);
    }
    //# Recover the Anthropic continuation owned by this provider
    let state = match (conversation) {
      let value: root.anthropic.internal.Conversation => value,
      _ => {
        throw root.ai.InvalidRequest {
          provider: "anthropic",
          status_code: null,
          detail: "Anthropic received another provider's conversation",
        }
      },
    };
    if (!root.ai.internal._same_provider_instance(state.owner, self)) {
      throw root.ai.InvalidRequest {
        provider: "anthropic",
        status_code: null,
        detail: "Anthropic received a conversation owned by another provider instance",
      };
    }
    if (
      state.last_assistant_content.some((part: json) -> bool {
        root.anthropic.internal._is_tool_use(part)
      })
    ) {
      throw root.ai.InvalidRequest {
        provider: "anthropic",
        status_code: null,
        detail: "submit tool results before requesting the next Anthropic step",
      };
    }
    //# Encode application tools independently from the typed output schema
    let encoded_tools = root.anthropic.internal._application_tools(
      tools,
      self.strict_tool_inputs,
    );
    let body = root.anthropic.internal._conversation_body<T>(
      self,
      state,
      encoded_tools,
    );
    log.info(
      baml.json.stringify({
        "event": "llm.tools.request",
        "provider": "anthropic",
        "model": self.model,
        "message_count": state.messages.length(),
        "tool_count": encoded_tools.length(),
        "tool_names": encoded_tools
          .map((tool: json) -> string {
            baml.json.path_or(tool, ".name", "")
          })
          .join(","),
        "output_mode": self.output_mode.to_string(),
        "tool_mode": self.tool_mode.to_string(),
        "parallel_tool_calls": self.parallel_tool_calls,
      }),
    );
    //# Authenticate and execute one stateless Messages turn
    let request = root.anthropic.internal._authenticated_continuation_request(
      self,
      state,
      body,
    );
    let response = baml.http.send(request) catch (e) {
      _ => {
        throw root.ai.NetworkFailure {
          provider: "anthropic",
          detail: "baml.http.send failed before an Anthropic Messages response arrived",
        }
      },
    };
    let text = response.text() catch (e) {
      _ => {
        throw root.ai.NetworkFailure {
          provider: "anthropic",
          detail: "the Anthropic Messages body could not be read completely",
        }
      },
    };
    if (!response.ok()) {
      throw root.ai.internal.classify_http(
        "anthropic",
        response.status_code,
        text,
      );
    }
    //# Decode the provider envelope without repairing wire errors
    let raw = baml.json.parse(text) catch (e) {
      _ => {
        throw root.ai.ParseFailed {
          provider: "anthropic",
          raw_output: text,
        }
      },
    };
    let stop_reason = baml.json.path_or(raw, ".stop_reason", "");
    if (stop_reason == "refusal") {
      throw root.ai.Refused {
        provider: "anthropic",
        reason: root.anthropic.internal._response_text(raw)
          ?? "Anthropic refused the request",
      };
    }
    if (
      stop_reason == "max_tokens"
        || stop_reason == "model_context_window_exceeded"
    ) {
      throw root.ai.ParseFailed {
        provider: "anthropic",
        raw_output: text,
      };
    }
    let raw_calls = root.anthropic.internal._response_tool_uses(raw);
    let metadata = root.anthropic.internal._response_metadata(raw, self.model);
    // Commit turn-local request state only after the request and provider
    // envelope have succeeded. Retry may call step again after a safe failure.
    state.active_tools = encoded_tools;
    //# Yield native tool_use blocks so the runner can execute them
    if (raw_calls.length() > 0) {
      if (stop_reason != "tool_use") {
        throw root.ai.ParseFailed {
          provider: "anthropic",
          raw_output: text,
        };
      }
      let content = root.anthropic.internal._response_content(raw);
      state.last_assistant_content = content;
      state.messages.push({
        "role": "assistant",
        "content": content,
      });
      state.history.entries.push(root.ai.ChatMessage {
        message_role: root.ai.MessageRole.Assistant,
        content: [root.ai.messages.ProviderDataPart {
          provider: "anthropic",
          data: { "content": content },
        }],
        metadata: null,
      });
      return root.ai.ModelStep<T> {
        outcome: root.anthropic.internal._application_calls(raw_calls, text),
        metadata: metadata,
      };
    }
    //# Parse a final strict JSON value or SAP text value
    let value = root.anthropic.internal._parse_response_value<T>(
      raw,
      text,
      root.anthropic.internal._uses_sap(self),
    );
    let content = root.anthropic.internal._response_content(raw);
    state.last_assistant_content = content;
    state.messages.push({
      "role": "assistant",
      "content": content,
    });
    state.history.entries.push(
      root.ai.ChatMessage.assistant(
        root.anthropic.internal._response_text(raw) ?? "",
      ),
    );
    root.ai.ModelStep<T> {
      outcome: value,
      metadata: metadata,
    }
  }

  function submit(
    self,
    conversation: root.ai.Conversation,
    results: root.ai.tools.ToolResult[],
  ) -> root.ai.Conversation
      throws root.ai.Failure {
    root.anthropic.internal._validate_output_config(self);
    if (self.tool_mode == root.anthropic.ToolMode.Prompt) {
      return root.anthropic.internal._prompt_submit(self, conversation, results);
    }
    //# Recover the continuation that produced these tool_use IDs
    let state = match (conversation) {
      let value: root.anthropic.internal.Conversation => value,
      _ => {
        throw root.ai.InvalidRequest {
          provider: "anthropic",
          status_code: null,
          detail: "Anthropic received another provider's conversation",
        }
      },
    };
    if (!root.ai.internal._same_provider_instance(state.owner, self)) {
      throw root.ai.InvalidRequest {
        provider: "anthropic",
        status_code: null,
        detail: "Anthropic received a conversation owned by another provider instance",
      };
    }
    //# Correlate exactly one runner result with every pending tool_use block
    let assistant_content = state.last_assistant_content;
    let pending: json[] = [];
    for (let part in assistant_content) {
      if (root.anthropic.internal._is_tool_use(part)) {
        pending.push(part);
      }
    }
    if (pending.length() == 0 || pending.length() != results.length()) {
      throw root.ai.InvalidRequest {
        provider: "anthropic",
        status_code: null,
        detail: "tool results must match every pending Anthropic tool_use block",
      };
    }
    // Validate and order the complete result set before mutating either
    // portable or provider-native conversation history.
    let ordered_results: root.ai.tools.ToolResult[] = [];
    for (let call in pending) {
      let call_id = root.anthropic.internal._tool_use_id(call) ?? "";
      let pending_with_id: json[] = [];
      for (let candidate in pending) {
        if ((root.anthropic.internal._tool_use_id(candidate) ?? "") == call_id) {
          pending_with_id.push(candidate);
        }
      }
      let matching: root.ai.tools.ToolResult[] = [];
      for (let candidate in results) {
        if (candidate.id == call_id) {
          matching.push(candidate);
        }
      }
      log.debug("call=" + call_id + " pending_same=" + pending_with_id.length().to_string() + " matching=" + matching.length().to_string());
      if (
        call_id.length() == 0
          || pending_with_id.length() != 1
          || matching.length() != 1
      ) {
        throw root.ai.InvalidRequest {
          provider: "anthropic",
          status_code: null,
          detail: "tool result IDs must match pending Anthropic tool_use IDs exactly once",
        };
      }
      ordered_results.push(matching[0]);
    }
    // All correlations are valid. Build the two history projections, then
    // commit them together so a late invalid ID cannot leave partial state.
    let tool_results: json[] = [];
    let history_entries: root.ai.Message[] = [];
    for (let result in ordered_results) {
      let result_block: json = {
        "type": "tool_result",
        "tool_use_id": result.id,
        "content": baml.json.stringify(result.output),
        "is_error": result.is_error,
      };
      tool_results.push(result_block);
      history_entries.push(root.ai.ChatMessage {
        message_role: root.ai.MessageRole.Tool,
        content: [root.ai.messages.ProviderDataPart {
          provider: "anthropic",
          data: {
            "tool_use_id": result.id,
            "output": result.output,
            "is_error": result.is_error,
          },
        }],
        metadata: null,
      });
    }
    for (let entry in history_entries) {
      state.history.entries.push(entry);
    }
    state.messages.push({
      "role": "user",
      "content": tool_results,
    });
    state.last_assistant_content = [];
    state
  }
}
