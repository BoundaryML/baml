implements Tools for OpenAi {
  type Transcript = OpenAiToolTranscript

  function begin<T>(self, request: Request<T>) -> OpenAiToolTranscript
      throws baml.errors.ToolError | baml.errors.UnknownError {
    let tools_json: json[] = request.tools.map((tool: Tool) -> json {
      let schema = openai_strict_schema(tool.parameters) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["tool schema generation failed"] },
      };
      {
        "type": "function",
        "function": {
          "name": tool.name,
          "description": tool.description,
          "parameters": schema,
          "strict": true,
        },
      }
    });
    OpenAiToolTranscript {
      model: self.model,
      api_key: self.api_key,
      base_url: self.base_url,
      tools: tools_json,
      messages: [{ "role": "user", "content": baml.llm.prompt_to_text(request.prompt) }],
      response_format: if (
        request.output_type() == reflect.type_of<string>()
        || request.output_type() == reflect.type_of<null>()
      ) {
        null
      } else {
        openai_chat_response_format(
          request.output_type(), request.identity?.name ?? "output",
        ) catch (e) {
          _ => throw baml.errors.UnknownError { data: e, message: ["final output schema generation failed"] },
        }
      },
    }
  }

  function step<T>(self, transcript: OpenAiToolTranscript) -> T | ToolCalls
      throws baml.errors.UnknownError {
    let body: map<string, json> = {
      "model": transcript.model,
      "messages": transcript.messages,
      "tools": transcript.tools,
      "tool_choice": "auto",
      "reasoning_effort": "none",
    };
    match (transcript.response_format) {
      let response_format: map<string, json> => { body.set("response_format", response_format); },
      null => {},
    }
    log.info(baml.json.stringify({
      "event": "llm.tools.request",
      "provider": "openai",
      "model": transcript.model,
      "body": baml.json.stringify(body),
    }));
    let response = baml.http.send(baml.http.Request {
      method: "POST",
      url: (transcript.base_url ?? "https://api.openai.com/v1") + "/chat/completions",
      headers: {
        "Authorization": "Bearer " + transcript.api_key,
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
      "model": transcript.model,
      "status_code": response.status_code,
      "body": text,
    }));
    let wire: OpenAiToolResponse = baml.sap.parse<OpenAiToolResponse>(text) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["openai tools parse failed"] },
    };
    let message = match (wire.choices.at(0)) {
      let choice: OpenAiToolChoice => choice.message,
      null => throw baml.errors.UnknownError { data: text, message: ["openai tools: no choices"] },
    };
    match (message.tool_calls) {
      let calls_json: json[] => {
        transcript.messages.push({ "role": "assistant", "tool_calls": calls_json });
        ToolCalls {
          calls: calls_json.map((call: json) -> ToolCall {
            ToolCall {
              id: baml.json.path_or(call, ".id", ""),
              name: baml.json.path_or(call, ".function.name", ""),
              args: baml.json.path_or(call, ".function.arguments", ""),
            }
          }),
        }
      },
      null => match (transcript.response_format) {
        let ignored: map<string, json> => baml.json.from_string<T>(message.content ?? "") catch (e) {
          _ => throw baml.errors.UnknownError { data: e, message: ["openai tools strict final parse failed"] },
        },
        null => if (reflect.type_of<T>() == reflect.type_of<null>()) {
          // A side-effect-only task has no model payload to decode. Reaching a
          // non-tool assistant turn is its successful completion signal.
          baml.json.from_string<T>("null") catch (e) {
            _ => throw baml.errors.UnknownError { data: e, message: ["openai tools null final decode failed"] },
          }
        } else {
          baml.sap.parse<T>(message.content ?? "") catch (e) {
            _ => throw baml.errors.UnknownError { data: e, message: ["openai tools final parse failed"] },
          }
        },
      },
    }
  }

  function submit(
    self,
    transcript: OpenAiToolTranscript,
    results: ToolResult[],
  ) -> OpenAiToolTranscript throws never {
    for (let result in results) {
      transcript.messages.push({
        "role": "tool",
        "tool_call_id": result.id,
        "content": result.output,
      });
    }
    transcript
  }
}
