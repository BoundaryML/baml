function _configured_primitive(
  shorthand: string,
  model: string,
  api_key: string?,
  base_url: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,
) -> baml.llm.PrimitiveClient throws baml.errors.UnknownError {
  let base = baml.llm.from_shorthand(shorthand) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["openai provider shorthand failed"],
    },
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
  task: root.ai.Task<T>,
  model: string,
) -> root.ai.ResponseWithMetadata<T> throws root.ai.Failure | baml.errors.UnknownError {
  let specialized = primitive.specialize_prompt(task.prompt) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["openai prompt specialization failed"],
    },
  };
  let request = primitive.build_request(specialized, task.output_type()) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["openai request build failed"],
    },
  };
  log.info(baml.json.stringify({
    "event": "llm.request",
    "provider": "openai",
    "model": model,
    // Never emit prompts or encoded media into ordinary integration logs.
    "request_bytes": request.body.length(),
  }));
  let http_response = baml.http.send(request) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["openai send failed"],
    },
  };
  let body = http_response.text() catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["openai body read failed"],
    },
  };
  log.info(baml.json.stringify({
    "event": "llm.response",
    "provider": "openai",
    "model": model,
    "status_code": http_response.status_code,
    "response_bytes": body.length(),
  }));
  if (!http_response.ok()) {
    //# A non-2xx status is a classified failure, not an unknown
    throw root.ai.internal.classify_http("openai", http_response.status_code, body);
  }
  let value = primitive.parse<T>(body) catch (e) {
    _ => throw root.ai.ParseFailed { provider: "openai", raw_output: body },
  };
  let raw: json = baml.json.parse(body) catch (e) { _ => null };
  root.ai.ResponseWithMetadata<T> {
    value: value,
    metadata: root.ai.ResponseMetadata {
      provider: "openai",
      model: model,
      request_id: baml.json.path_or<string?>(raw, ".id", null),
      finish_reason: baml.json.path_or<string?>(raw, ".choices[0].finish_reason", null),
      usage: root.ai.Usage {
        input_tokens: baml.json.path_or(raw, ".usage.prompt_tokens", 0),
        output_tokens: baml.json.path_or(raw, ".usage.completion_tokens", 0),
        cached_input_tokens: baml.json.path_or<int?>(raw, ".usage.prompt_tokens_details.cached_tokens", null),
        reasoning_tokens: baml.json.path_or<int?>(
          raw,
          ".usage.completion_tokens_details.reasoning_tokens",
          null,
        ),
        cost_usd: null,
      },
      attributes: {},
      raw: raw,
    },
    conversation: null,
  }
}

function _primitive_stream<TPartial, T>(
  primitive: baml.llm.PrimitiveClient,
  task: root.ai.Task<T>,
) -> baml.llm.Stream<TPartial, T> throws baml.errors.UnknownError {
  let specialized = primitive.specialize_prompt(task.prompt) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["openai stream prompt failed"],
    },
  };
  let request = primitive.build_request_stream(specialized, task.output_type()) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["openai stream request failed"],
    },
  };
  log.info(baml.json.stringify({
    "event": "llm.stream.request",
    "provider": "openai",
    "method": request.method,
    "url": request.url,
    // Prompts and encoded media must not leak into routine integration logs.
    "request_bytes": request.body.length(),
  }));
  let sse = baml.http.fetch_sse(request) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["openai stream open failed"],
    },
  };
  let accumulator = primitive.new_stream_accumulator() catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["openai stream accumulator failed"],
    },
  };
  baml.llm.Stream<TPartial, T> {
    _client: primitive,
    _acc: accumulator,
    _sse: sse,
    _cache: baml.llm.__new_stream_cache<TPartial, T>(),
  }
}
