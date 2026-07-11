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
  let request_id: string? = match (raw) {
    null => null,
    let value: json => baml.json.path_or<string?>(value, ".id", null),
  };
  let finish_reason: string? = match (raw) {
    null => null,
    let value: json => {
      if (provider_name == "anthropic") {
        baml.json.path_or<string?>(value, ".stop_reason", null)
      } else if (provider_name == "gemini") {
        baml.json.path_or<string?>(value, ".candidates[0].finishReason", null)
      } else if (provider_name == "openai-responses") {
        baml.json.path_or<string?>(value, ".status", null)
      } else {
        baml.json.path_or<string?>(value, ".choices[0].finish_reason", null)
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
