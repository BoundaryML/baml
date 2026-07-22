function configured_primitive(
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

function primitive_generate<T>(
  primitive: baml.llm.PrimitiveClient,
  task: Task<T>,
  provider_name: string,
  model: string,
) -> Response<T> throws baml.errors.UnknownError {
  let specialized = primitive.specialize_prompt(task.prompt) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " prompt specialization failed"] },
  };
  let request = primitive.build_request(specialized, task.output_type()) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " request build failed"] },
  };
  log.info(baml.json.stringify({
    "event": "llm.request",
    "provider": provider_name,
    "model": model,
    // Never emit prompts or encoded media into ordinary integration logs.
    "request_bytes": request.body.length(),
  }));
  let http_response = baml.http.send(request) catch (e) {
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
    "response_bytes": body.length(),
  }));
  if (!http_response.ok()) {
    throw baml.errors.UnknownError {
      data: body,
      message: [provider_name + " http " + http_response.status_code.to_string()],
    };
  }
  let value = primitive.parse<T>(body) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " typed parse failed"] },
  };
  let raw: json = baml.json.parse(body) catch (e) { _ => null };
  let usage = if (provider_name == "anthropic") {
    Usage {
      input_tokens: baml.json.path_or(raw, ".usage.input_tokens", 0),
      output_tokens: baml.json.path_or(raw, ".usage.output_tokens", 0),
      cached_input_tokens: baml.json.path_or<int?>(raw, ".usage.cache_read_input_tokens", null),
      reasoning_tokens: null,
      cost_usd: null,
    }
  } else {
    Usage {
      input_tokens: baml.json.path_or(raw, ".usage.prompt_tokens", 0),
      output_tokens: baml.json.path_or(raw, ".usage.completion_tokens", 0),
      cached_input_tokens: baml.json.path_or<int?>(raw, ".usage.prompt_tokens_details.cached_tokens", null),
      reasoning_tokens: baml.json.path_or<int?>(raw, ".usage.completion_tokens_details.reasoning_tokens", null),
      cost_usd: null,
    }
  };
  Response<T> {
    value: value,
    meta: Meta {
      provider: provider_name,
      model: model,
      request_id: baml.json.path_or<string?>(raw, ".id", null),
      finish_reason: if (provider_name == "anthropic") {
        baml.json.path_or<string?>(raw, ".stop_reason", null)
      } else {
        baml.json.path_or<string?>(raw, ".choices[0].finish_reason", null)
      },
      usage: usage,
      attributes: {},
      raw: raw,
    },
    conversation: null,
  }
}
