function _primitive_generate<T>(
  primitive: baml.llm.PrimitiveClient,
  request: Request<T>,
  provider_name: string,
  model: string,
  body_overrides: map<string, json>? = null,
  strict_json_output: bool = false,
  strict_tool_output_name: string? = null,
) -> Response<T> throws baml.errors.UnknownError {
  let specialized = primitive.specialize_prompt(request.prompt) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " prompt specialization failed"] },
  };
  let built_request = primitive.build_request(specialized, request.output_type()) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " request build failed"] },
  };
  let http_request = match (body_overrides) {
    null => built_request,
    let overrides: map<string, json> => {
      let body = baml.json.parse(built_request.body) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: [provider_name + " request body was not JSON"],
        },
      };
      let object = match (body) {
        let value: map<string, json> => value,
        _ => throw baml.errors.UnknownError {
          data: body,
          message: [provider_name + " request body was not an object"],
        },
      };
      for (let key in overrides.keys()) {
        object.set(key, overrides[key]);
      }
      baml.http.Request {
        method: built_request.method,
        url: built_request.url,
        headers: built_request.headers,
        body: baml.json.stringify(object),
      }
    },
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
  let raw: json? = baml.json.parse(body) catch (e) { _ => null };
  let value: T = match (strict_tool_output_name) {
    let output_tool_name: string => {
      let envelope = match (raw) {
        null => throw baml.errors.UnknownError {
          data: body,
          message: [provider_name + " response was not JSON"],
        },
        let value: json => value,
      };
      let refusal = baml.json.path_or<string?>(envelope, ".choices[0].message.refusal", null);
      match (refusal) {
        let message: string => throw baml.errors.UnknownError {
          data: envelope,
          message: [provider_name + " refused: " + message],
        },
        null => {},
      }
      let call = baml.json.path<json>(envelope, ".choices[0].message.tool_calls[0]") catch (e) {
        _ => throw baml.errors.UnknownError {
          data: envelope,
          message: [provider_name + " response had no output tool call"],
        },
      };
      let actual_name = baml.json.path_or<string>(call, ".function.name", "");
      if (actual_name != output_tool_name) {
        throw baml.errors.UnknownError {
          data: call,
          message: [provider_name + " called unexpected output tool " + actual_name],
        };
      }
      let arguments = baml.json.path<string>(call, ".function.arguments") catch (e) {
        _ => throw baml.errors.UnknownError {
          data: call,
          message: [provider_name + " output tool had no arguments"],
        },
      };
      let wrapper = baml.json.parse(arguments) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: [provider_name + " output tool arguments were not JSON"],
        },
      };
      let output = baml.json.path<json>(wrapper, ".value") catch (e) {
        _ => throw baml.errors.UnknownError {
          data: wrapper,
          message: [provider_name + " output tool arguments had no value"],
        },
      };
      baml.json.from_string<T>(baml.json.stringify(output)) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: [provider_name + " strict output tool decode failed"],
        },
      }
    },
    null => if (strict_json_output) {
    let envelope = match (raw) {
      null => throw baml.errors.UnknownError {
        data: body,
        message: [provider_name + " response was not JSON"],
      },
      let value: json => value,
    };
    let refusal = baml.json.path_or<string?>(envelope, ".choices[0].message.refusal", null);
    match (refusal) {
      let message: string => throw baml.errors.UnknownError {
        data: envelope,
        message: [provider_name + " refused: " + message],
      },
      null => {},
    }
    let content = baml.json.path<string>(envelope, ".choices[0].message.content") catch (e) {
      _ => throw baml.errors.UnknownError {
        data: envelope,
        message: [provider_name + " response had no message content"],
      },
    };
    baml.json.from_string<T>(content) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: [provider_name + " strict JSON decode failed"],
      },
    }
    } else {
    primitive.parse<T>(body) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " typed parse failed"] },
    }
    },
  };
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
