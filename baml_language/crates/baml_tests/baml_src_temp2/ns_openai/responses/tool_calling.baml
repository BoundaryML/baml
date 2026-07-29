implements root.ai.tools.ToolCallingProvider for Responses {
  function begin<T>(
    self,
    task: root.ai.Task<T>,
  ) -> root.ai.Conversation
      throws root.ai.Failure | baml.errors.UnknownError {
    root.openai.internal._validate_output_config(self);
    if (self.tool_mode == root.openai.ToolMode.Prompt) {
      // Prompt mode is selected here so OpenAI users configure one provider;
      // the generic adapter remains an internal implementation detail.
      let prompt_tools = root.ai.tools.PromptToolProvider { inner: self };
      return prompt_tools.begin<T>(task);
    }
    //# Render the first turn into Responses input items
    let request = root.openai.internal._responses_built_request(
      self,
      task,
      false,
      log_shape = false,
    );
    let request_body = baml.json.parse(request.body) catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(
        e,
        "OpenAI Responses initial request body was not JSON",
      ),
    };
    root.openai.internal.Conversation {
      owner: self,
      history: task.messages().to_history(),
      input: baml.json.path_or<json[]>(request_body, ".input", []),
      previous_response_id: null,
      active_tools: [],
      last_output: [],
    }
  }

  function step<T>(
    self,
    conversation: root.ai.Conversation,
    tools: root.ai.tools.Tool[],
  ) -> root.ai.tools.ModelStep<T>
      throws root.ai.Failure | baml.errors.UnknownError {
    root.openai.internal._validate_output_config(self);
    if (self.tool_mode == root.openai.ToolMode.Prompt) {
      let prompt_tools = root.ai.tools.PromptToolProvider { inner: self };
      return prompt_tools.step<T>(conversation, tools);
    }
    let strict = root.openai.internal._is_strict(self);
    let use_sap = root.openai.internal._uses_sap(self);
    //# Recover the Responses continuation owned by this provider
    let state = match (conversation) {
      let value: root.openai.internal.Conversation => value,
      _ => throw root.ai.InvalidRequest {
        provider: "openai-responses",
        status_code: null,
        detail: "OpenAI Responses received another provider's conversation",
      },
    };
    //# Select native or hybrid SAP tool transport
    // With OutputMode.Sap, _agent_tools keeps native application tools but
    // omits the result tool; final T comes from output_text. ToolMode.Prompt
    // returned through the prompt adapter above and never reaches this path.
    let encoded_tools = root.openai.internal._agent_tools(
      reflect.type_of<T>(),
      tools,
      strict,
      use_sap,
    );
    state.active_tools = encoded_tools;
    let body = root.openai.internal._request_body(self, state.input);
    let allow_parallel = root.openai.internal._allow_parallel_tool_calls(
      self,
      tools.length(),
    );
    body.set("store", true);
    match (state.previous_response_id) {
      let id: string => {
        body.set("previous_response_id", id);
      },
      null => {},
    }
    if (encoded_tools.length() > 0) {
      body.set("tools", encoded_tools);
      body.set("tool_choice", if (use_sap) { "auto" } else { "required" });
      // Only application-tool fanout controls this flag. T and T[] use the
      // same singular result tool.
      body.set(
        "parallel_tool_calls",
        allow_parallel,
      );
    }
    log.info(baml.json.stringify({
      "event": "llm.tools.request",
      "provider": "openai-responses",
      "model": self.model,
      "input_count": state.input.length(),
      "tool_count": encoded_tools.length(),
      "tool_names": encoded_tools
        .map((tool: json) -> string {
          baml.json.path_or(tool, ".name", "")
        })
        .join(","),
      "output_mode": self.output_mode.to_string(),
      "tool_mode": self.tool_mode.to_string(),
      "parallel_tool_calls": allow_parallel,
    }));
    //# Execute one Responses model turn
    let response = baml.http.send(baml.http.Request {
      method: "POST",
      url: (self.base_url ?? "https://api.openai.com/v1") + "/responses",
      headers: root.openai.internal._request_headers(self),
      body: baml.json.stringify(body),
    }) catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(
        e,
        "OpenAI Responses tools send failed",
      ),
    };
    let text = response.text() catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(
        e,
        "OpenAI Responses tools body failed",
      ),
    };
    if (!response.ok()) {
      throw root.ai.internal.classify_http(
        "openai-responses",
        response.status_code,
        text,
      );
    }
    //# Decode the provider envelope without repairing wire errors
    let raw = baml.json.parse(text) catch (e) {
      _ => throw root.ai.ParseFailed {
        provider: "openai-responses",
        raw_output: text,
      },
    };
    match (root.openai.internal._response_refusal(raw)) {
      let refusal: string => throw root.ai.Refused {
        provider: "openai-responses",
        reason: refusal,
      },
      null => {},
    }
    let status = baml.json.path_or(raw, ".status", "");
    if (status == "failed" || status == "cancelled" || status == "incomplete") {
      throw root.ai.ParseFailed {
        provider: "openai-responses",
        raw_output: text,
      };
    }
    let calls = root.openai.internal._response_function_calls(raw);
    let output_calls = calls.filter((call: json) -> bool {
      baml.json.path_or(call, ".name", "") == root.openai.internal._output_tool_name()
    });
    let metadata = root.openai.internal._response_metadata(raw, self.model);
    //# Finish only when the singular reserved result tool is called
    if (!use_sap && output_calls.length() > 0) {
      if (calls.length() != 1 || output_calls.length() != 1) {
        // A final result cannot be mixed with application side effects.
        throw root.ai.ParseFailed {
          provider: "openai-responses",
          raw_output: text,
        };
      }
      let value = root.openai.internal._parse_output_call<T>(
        output_calls[0],
        text,
      );
      state.history.entries.push(
        root.ai.ChatMessage.assistant(
          baml.json.stringify(
            root.openai.internal._output_call_value(output_calls[0], text),
          ),
        ),
      );
      return root.ai.tools.ModelStep<T> {
        outcome: value,
        metadata: metadata,
      };
    }
    //# Yield application calls so the runner can execute them
    if (calls.length() > 0) {
      state.last_output = calls;
      let response_id = baml.json.path_or(raw, ".id", "");
      if (response_id.length() == 0) {
        throw root.ai.ParseFailed {
          provider: "openai-responses",
          raw_output: text,
        };
      }
      state.previous_response_id = response_id;
      state.input = [];
      state.history.entries.push(root.ai.ChatMessage {
        message_role: root.ai.MessageRole.Assistant,
        content: [root.ai.messages.ProviderDataPart {
          provider: "openai-responses",
          data: { "output": calls },
        }],
        metadata: null,
      });
      return root.ai.tools.ModelStep<T> {
        outcome: root.openai.internal._application_calls(calls, text),
        metadata: metadata,
      };
    }
    //# Hybrid SAP mode accepts final T as output_text
    // Calls above are still native application calls. If T is a union, SAP
    // chooses among those T branches after the application-call branch is
    // ruled out by the Responses envelope.
    if (!use_sap) {
      throw root.ai.ParseFailed {
        provider: "openai-responses",
        raw_output: text,
      };
    }
    let content = match (root.openai.internal._response_output_text(raw)) {
      let value: string => value,
      null => throw root.ai.ParseFailed {
        provider: "openai-responses",
        raw_output: text,
      },
    };
    state.history.entries.push(root.ai.ChatMessage.assistant(content));
    let value = baml.sap.parse<T>(content) catch (e) {
      _ => throw root.ai.ParseFailed {
        provider: "openai-responses",
        raw_output: content,
      },
    };
    root.ai.tools.ModelStep<T> {
      outcome: value,
      metadata: metadata,
    }
  }

  function submit(
    self,
    conversation: root.ai.Conversation,
    results: root.ai.tools.ToolResult[],
  ) -> root.ai.Conversation throws root.ai.Failure {
    root.openai.internal._validate_output_config(self);
    if (self.tool_mode == root.openai.ToolMode.Prompt) {
      let prompt_tools = root.ai.tools.PromptToolProvider { inner: self };
      return prompt_tools.submit(conversation, results);
    }
    //# Recover the continuation that produced these call IDs
    let state = match (conversation) {
      let value: root.openai.internal.Conversation => value,
      _ => throw root.ai.InvalidRequest {
        provider: "openai-responses",
        status_code: null,
        detail: "OpenAI Responses received another provider's conversation",
      },
    };
    //# Correlate runner results with Responses function_call_output items
    state.input = results.map((result: root.ai.tools.ToolResult) -> json {
      {
        "type": "function_call_output",
        "call_id": result.id,
        "output": baml.json.stringify(result.output),
      }
    });
    for (let result in results) {
      state.history.entries.push(root.ai.ChatMessage {
        message_role: root.ai.MessageRole.Tool,
        content: [root.ai.messages.ProviderDataPart {
          provider: "openai-responses",
          data: {
            "call_id": result.id,
            "output": result.output,
            "is_error": result.is_error,
          },
        }],
        metadata: null,
      });
    }
    state
  }
}
