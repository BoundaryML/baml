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
