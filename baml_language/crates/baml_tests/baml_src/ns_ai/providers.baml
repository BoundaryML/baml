// Live provider reference implementations.  Wire construction/parsing is
// delegated to the existing low-level baml.llm PrimitiveClient host seam; the
// public provider/request/capability surface is root.ai only.

function _configured_primitive(
  shorthand: string,
  model: string,
  api_key: string?,
  base_url: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,
) -> baml.llm.PrimitiveClient throws baml.errors.UnknownError {
  let base = baml.llm.from_shorthand(shorthand) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: ["provider shorthand failed"] },
  };
  baml.llm.PrimitiveClient {
    ...base,
    options: baml.llm.PrimitiveClientOptions {
      ...base.options,
      model: model,
      api_key: api_key,
      base_url: base_url,
      headers: extra_headers ?? base.options.headers,
      request_body: extra_body ?? base.options.request_body,
    },
  }
}

function _primitive_generate<T>(
  primitive: baml.llm.PrimitiveClient,
  request: Request<T>,
  provider_name: string,
  model: string,
) -> Response<T> throws baml.errors.UnknownError {
  let specialized = primitive.specialize_prompt(request.prompt) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " prompt specialization failed"] },
  };
  let http_request = primitive.build_request(specialized, request.output_type()) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " request build failed"] },
  };
  log.info(baml.json.stringify({
    "event": "llm.request",
    "provider": provider_name,
    "model": model,
    "body": http_request.body,
  }));
  let http_response = baml.http.send(http_request) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " send failed"] },
  };
  let body = http_response.text() catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " body read failed"] },
  };
  log.info(baml.json.stringify({
    "event": "llm.response",
    "provider": provider_name,
    "model": model,
    "status_code": http_response.status_code,
    "body": body,
  }));
  if (!http_response.ok()) {
    throw baml.errors.UnknownError {
      data: body,
      message: [provider_name + " http " + http_response.status_code.to_string()],
    };
  }
  let value: T = primitive.parse<T>(body) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " typed parse failed"] },
  };
  let raw: json? = baml.json.parse(body) catch (e) { _ => null };
  let usage = match (raw) {
    null => null,
    let value: json => {
      if (provider_name == "anthropic") {
        Usage {
          input_tokens: baml.json.path_or(value, ".usage.input_tokens", 0),
          output_tokens: baml.json.path_or(value, ".usage.output_tokens", 0),
        }
      } else if (provider_name == "gemini") {
        Usage {
          input_tokens: baml.json.path_or(value, ".usageMetadata.promptTokenCount", 0),
          output_tokens: baml.json.path_or(value, ".usageMetadata.candidatesTokenCount", 0),
        }
      } else if (provider_name == "openai-responses") {
        Usage {
          input_tokens: baml.json.path_or(value, ".usage.input_tokens", 0),
          output_tokens: baml.json.path_or(value, ".usage.output_tokens", 0),
        }
      } else {
        Usage {
          input_tokens: baml.json.path_or(value, ".usage.prompt_tokens", 0),
          output_tokens: baml.json.path_or(value, ".usage.completion_tokens", 0),
        }
      }
    },
  };
  let request_id = match (raw) {
    null => null,
    let value: json => baml.json.path_or(value, ".id", null),
  };
  let finish_reason = match (raw) {
    null => null,
    let value: json => {
      if (provider_name == "anthropic") {
        baml.json.path_or(value, ".stop_reason", null)
      } else if (provider_name == "gemini") {
        baml.json.path_or(value, ".candidates[0].finishReason", null)
      } else if (provider_name == "openai-responses") {
        baml.json.path_or(value, ".status", null)
      } else {
        baml.json.path_or(value, ".choices[0].finish_reason", null)
      }
    },
  };
  Response<T> {
    value: value,
    meta: Meta {
      provider: provider_name,
      model: model,
      request_id: request_id,
      finish_reason: finish_reason,
      usage: usage,
      attributes: {},
      raw: raw,
    },
  }
}

function _primitive_stream<TPartial, T>(
  primitive: baml.llm.PrimitiveClient,
  request: Request<T>,
  provider_name: string,
) -> baml.llm.Stream<TPartial, T> throws baml.errors.UnknownError {
  let specialized = primitive.specialize_prompt(request.prompt) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " stream prompt failed"] },
  };
  let http_request = primitive.build_request_stream(specialized, request.output_type()) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " stream request failed"] },
  };
  log.info(baml.json.stringify({
    "event": "llm.stream.request",
    "provider": provider_name,
    "body": http_request.body,
  }));
  let sse = baml.http.fetch_sse(http_request) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " stream open failed"] },
  };
  let accumulator = primitive.new_stream_accumulator() catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " accumulator failed"] },
  };
  let cache = baml.llm.StreamCache.new(reflect.type_of<TPartial>(), reflect.type_of<T>());
  baml.llm.Stream<TPartial, T> {
    _client: primitive,
    _acc: accumulator,
    _sse: sse,
    _cache: cache,
  }
}

enum Support {
  Yes
  No
  Conditional
}

interface Capabilities requires Provider {
  function structured_output(self) -> Support throws never
  function image_input(self) -> Support throws never
  function parallel_tools(self) -> Support throws never
  function max_input_tokens(self) -> int throws never
}

class OpenAi {
  model: string,
  api_key: string,
  base_url: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,
  append_output_schema: bool?,

  implements Provider {
    function display_name(self) -> string throws never { "openai" }
    function render_shorthand(self) -> string throws never { "openai/" + self.model }
  }

  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T>
        throws baml.errors.UnknownError {
      _primitive_generate<T>(
        _configured_primitive(self.render_shorthand(), self.model, self.api_key, self.base_url, self.extra_headers, self.extra_body),
        request, "openai", self.model,
      )
    }
  }

  implements Streaming {
    function stream<TPartial, T>(self, request: Request<T>) -> baml.llm.Stream<TPartial, T>
        throws baml.errors.UnknownError {
      _primitive_stream<TPartial, T>(
        _configured_primitive(self.render_shorthand(), self.model, self.api_key, self.base_url, self.extra_headers, self.extra_body),
        request, "openai",
      )
    }
  }

  implements Capabilities {
    function structured_output(self) -> Support throws never { Support.Yes }
    function image_input(self) -> Support throws never { Support.Yes }
    function parallel_tools(self) -> Support throws never { Support.Yes }
    function max_input_tokens(self) -> int throws never { 128000 }
  }

  implements Tools {
    type Transcript = OpenAiToolTranscript

    function begin<T>(self, request: Request<T>) -> OpenAiToolTranscript
        throws baml.errors.ToolError | baml.errors.UnknownError {
      let tools_json: json[] = request.tools.map((tool: Tool) -> json {
        let schema_text = baml.schema.json_schema(tool.parameters, false) catch (e) {
          _ => throw baml.errors.UnknownError { data: e, message: ["tool schema generation failed"] },
        };
        let schema = baml.json.parse(schema_text) catch (e) {
          _ => throw baml.errors.UnknownError { data: e, message: ["tool schema JSON was invalid"] },
        };
        {
          "type": "function",
          "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": schema,
          },
        }
      });
      OpenAiToolTranscript {
        model: self.model,
        api_key: self.api_key,
        base_url: self.base_url,
        tools: tools_json,
        messages: [{ "role": "user", "content": baml.llm.prompt_to_text(request.prompt) }],
      }
    }

    function step<T>(self, transcript: OpenAiToolTranscript) -> T | ToolCalls
        throws baml.errors.UnknownError {
      let body: json = {
        "model": transcript.model,
        "messages": transcript.messages,
        "tools": transcript.tools,
        "tool_choice": "auto",
        "reasoning_effort": "none",
      };
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
        null => baml.sap.parse<T>(message.content ?? "") catch (e) {
          _ => throw baml.errors.UnknownError { data: e, message: ["openai tools final parse failed"] },
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
}

class Anthropic {
  model: string,
  api_key: string?,
  base_url: string?,
  max_tokens: int?,
  anthropic_version: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,
  append_output_schema: bool?,

  implements Provider {
    function display_name(self) -> string throws never { "anthropic" }
    function render_shorthand(self) -> string throws never { "anthropic/" + self.model }
  }
  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T> throws baml.errors.UnknownError {
      _primitive_generate<T>(
        _configured_primitive(self.render_shorthand(), self.model, self.api_key, self.base_url, self.extra_headers, self.extra_body),
        request, "anthropic", self.model,
      )
    }
  }
  implements Streaming {
    function stream<TPartial, T>(self, request: Request<T>) -> baml.llm.Stream<TPartial, T>
        throws baml.errors.UnknownError {
      _primitive_stream<TPartial, T>(
        _configured_primitive(self.render_shorthand(), self.model, self.api_key, self.base_url, self.extra_headers, self.extra_body),
        request, "anthropic",
      )
    }
  }
  implements Capabilities {
    function structured_output(self) -> Support throws never { Support.Conditional }
    function image_input(self) -> Support throws never { Support.Yes }
    function parallel_tools(self) -> Support throws never { Support.Yes }
    function max_input_tokens(self) -> int throws never { 200000 }
  }
}

class Gemini {
  model: string,
  api_key: string?,
  base_url: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,
  append_output_schema: bool?,

  implements Provider {
    function display_name(self) -> string throws never { "gemini" }
    function render_shorthand(self) -> string throws never { "google-ai/" + self.model }
  }
  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T> throws baml.errors.UnknownError {
      _primitive_generate<T>(
        _configured_primitive(self.render_shorthand(), self.model, self.api_key, self.base_url, self.extra_headers, self.extra_body),
        request, "gemini", self.model,
      )
    }
  }
  implements Streaming {
    function stream<TPartial, T>(self, request: Request<T>) -> baml.llm.Stream<TPartial, T>
        throws baml.errors.UnknownError {
      _primitive_stream<TPartial, T>(
        _configured_primitive(self.render_shorthand(), self.model, self.api_key, self.base_url, self.extra_headers, self.extra_body),
        request, "gemini",
      )
    }
  }
  implements Capabilities {
    function structured_output(self) -> Support throws never { Support.Yes }
    function image_input(self) -> Support throws never { Support.Yes }
    function parallel_tools(self) -> Support throws never { Support.Yes }
    function max_input_tokens(self) -> int throws never { 1000000 }
  }
}

class NoAuth {}
class BearerAuth { token: string }
class HeaderAuth { name: string, value: string }
type Auth = NoAuth | BearerAuth | HeaderAuth

class OpenAiCompatible {
  base_url: string,
  model: string,
  auth: Auth,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,
  append_output_schema: bool?,

  implements Provider {
    function display_name(self) -> string throws never { "openai-compatible" }
    function render_shorthand(self) -> string throws never { "openai/" + self.model }
  }

  function _api_key(self) -> string? throws never {
    match (self.auth) {
      let bearer: BearerAuth => bearer.token,
      _ => null,
    }
  }

  function _headers(self) -> map<string, string> throws never {
    let headers = self.extra_headers ?? {};
    match (self.auth) {
      let custom: HeaderAuth => { let _ = headers.set(custom.name, custom.value); },
      _ => {},
    }
    headers
  }

  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T> throws baml.errors.UnknownError {
      _primitive_generate<T>(
        _configured_primitive(
          self.render_shorthand(), self.model, self._api_key(), self.base_url,
          self._headers(), self.extra_body,
        ),
        request, "openai-compatible", self.model,
      )
    }
  }

  implements Streaming {
    function stream<TPartial, T>(self, request: Request<T>) -> baml.llm.Stream<TPartial, T>
        throws baml.errors.UnknownError {
      _primitive_stream<TPartial, T>(
        _configured_primitive(
          self.render_shorthand(), self.model, self._api_key(), self.base_url,
          self._headers(), self.extra_body,
        ),
        request, "openai-compatible",
      )
    }
  }
}

class OpenAiStrict {
  model: string,
  api_key: string,
  base_url: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,

  implements Provider {
    function display_name(self) -> string throws never { "openai-strict" }
    function render_shorthand(self) -> string throws never { "openai/" + self.model }
  }
  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T> throws baml.errors.UnknownError {
      _primitive_generate<T>(
        _configured_primitive(self.render_shorthand(), self.model, self.api_key, self.base_url, self.extra_headers, self.extra_body),
        request, "openai-strict", self.model,
      )
    }
  }

}

class OpenAiResponses {
  model: string,
  api_key: string,
  base_url: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,
  append_output_schema: bool?,
  store: bool?,

  implements Provider {
    function display_name(self) -> string throws never { "openai-responses" }
    function render_shorthand(self) -> string throws never { "openai-responses/" + self.model }
  }
  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T> throws baml.errors.UnknownError {
      _primitive_generate<T>(
        _configured_primitive(self.render_shorthand(), self.model, self.api_key, self.base_url, self.extra_headers, self.extra_body),
        request, "openai-responses", self.model,
      )
    }
  }

  implements Background {
    function submit<T>(self, request: Request<T>, options: BackgroundOptions) -> Job<T>
        throws baml.errors.UnknownError {
      let primitive = _configured_primitive(
        self.render_shorthand(), self.model, self.api_key, self.base_url,
        self.extra_headers, { "background": true, "store": true },
      );
      let specialized = primitive.specialize_prompt(request.prompt) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["background prompt failed"] },
      };
      let built = primitive.build_request(specialized, request.output_type()) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["background request failed"] },
      };
      let headers = built.headers;
      match (options.idempotency_key) {
        let key: string => { let _ = headers.set("Idempotency-Key", key); },
        null => {},
      }
      let response = baml.http.send(baml.http.Request {
        method: built.method, url: built.url, headers: headers, body: built.body,
      }) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["background submit failed"] },
      };
      let body = response.text() catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["background submit body failed"] },
      };
      let raw = baml.json.parse(body) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["background submit JSON failed"] },
      };
      OpenAiResponseJob<T> { owner: self, id: baml.json.path_or(raw, ".id", "") }
    }

    function resume_job<T>(self, token: JobToken) -> Job<T> throws baml.errors.UnknownError {
      if (token.provider != "openai-responses") {
        throw baml.errors.UnknownError { data: token, message: ["job token provider mismatch"] };
      }
      OpenAiResponseJob<T> { owner: self, id: token.id }
    }
  }

  implements Sessions {
    function open_session(self, options: SessionOptions) -> Session throws never {
      OpenAiSession { owner: self, id: "session", previous_response_id: null }
    }
    function resume_session(self, token: SessionToken) -> Session throws baml.errors.UnknownError {
      if (token.provider != "openai-responses") {
        throw baml.errors.UnknownError { data: token, message: ["session token provider mismatch"] };
      }
      OpenAiSession { owner: self, id: token.id, previous_response_id: token.id }
    }
  }
}

class OpenAiToolMessage {
  content: string?,
  tool_calls: json[]?,
}
class OpenAiToolChoice { message: OpenAiToolMessage }
class OpenAiToolResponse { choices: OpenAiToolChoice[] }
class OpenAiToolTranscript {
  model: string,
  api_key: string,
  base_url: string?,
  tools: json[],
  messages: json[],
}
