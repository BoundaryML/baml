function configured_primitive(
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
      message: ["anthropic provider shorthand failed"],
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

function primitive_generate<T>(
  primitive: baml.llm.PrimitiveClient,
  task: root.ai.Task<T>,
  model: string,
) -> root.ai.Response<T> throws baml.errors.UnknownError {
  let specialized = primitive.specialize_prompt(task.prompt) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["anthropic prompt specialization failed"],
    },
  };
  let request = primitive.build_request(specialized, task.output_type()) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["anthropic request build failed"],
    },
  };
  log.info(baml.json.stringify({
    "event": "llm.request",
    "provider": "anthropic",
    "model": model,
    "request_bytes": request.body.length(),
  }));
  let http_response = baml.http.send(request) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["anthropic send failed"],
    },
  };
  let body = http_response.text() catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["anthropic body read failed"],
    },
  };
  log.info(baml.json.stringify({
    "event": "llm.response",
    "provider": "anthropic",
    "model": model,
    "status_code": http_response.status_code,
    "response_bytes": body.length(),
  }));
  if (!http_response.ok()) {
    throw baml.errors.UnknownError {
      data: body,
      message: ["anthropic http " + http_response.status_code.to_string()],
    };
  }
  let value = primitive.parse<T>(body) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["anthropic typed parse failed"],
    },
  };
  let raw: json = baml.json.parse(body) catch (e) { _ => null };
  root.ai.Response<T> {
    value: value,
    meta: root.ai.Meta {
      provider: "anthropic",
      model: model,
      request_id: baml.json.path_or<string?>(raw, ".id", null),
      finish_reason: baml.json.path_or<string?>(raw, ".stop_reason", null),
      usage: root.ai.Usage {
        input_tokens: baml.json.path_or(raw, ".usage.input_tokens", 0),
        output_tokens: baml.json.path_or(raw, ".usage.output_tokens", 0),
        cached_input_tokens: baml.json.path_or<int?>(raw, ".usage.cache_read_input_tokens", null),
        reasoning_tokens: null,
        cost_usd: null,
      },
      attributes: {},
      raw: raw,
    },
    conversation: null,
  }
}

function primitive_stream<TPartial, T>(
  primitive: baml.llm.PrimitiveClient,
  task: root.ai.Task<T>,
) -> baml.llm.Stream<TPartial, T> throws baml.errors.UnknownError {
  let specialized = primitive.specialize_prompt(task.prompt) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["anthropic stream prompt failed"],
    },
  };
  let request = primitive.build_request_stream(specialized, task.output_type()) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["anthropic stream request failed"],
    },
  };
  log.info(baml.json.stringify({
    "event": "llm.stream.request",
    "provider": "anthropic",
    "method": request.method,
    "url": request.url,
    "request_bytes": request.body.length(),
  }));
  let sse = baml.http.fetch_sse(request) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["anthropic stream open failed"],
    },
  };
  let accumulator = primitive.new_stream_accumulator() catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["anthropic stream accumulator failed"],
    },
  };
  baml.llm.Stream<TPartial, T> {
    _client: primitive,
    _acc: accumulator,
    _sse: sse,
    _cache: baml.llm.__new_stream_cache<TPartial, T>(),
  }
}
