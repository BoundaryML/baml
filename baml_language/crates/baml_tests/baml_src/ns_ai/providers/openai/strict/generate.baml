implements Generate for OpenAiStrict {
  function generate<T>(self, request: Request<T>) -> Response<T> throws baml.errors.UnknownError {
    let output_tool_name = "__baml_return_output";
    let output_tool = openai_strict_output_tool(request.output_type(), output_tool_name) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["openai-strict output schema is unsupported"],
      },
    };
    _primitive_generate<T>(
      _configured_primitive(
        self.render_shorthand(), self.model, self.api_key, self.base_url,
        self.extra_headers, self.extra_body,
      ),
      request, "openai-strict", self.model,
      body_overrides = {
        "tools": [output_tool],
        "tool_choice": {
          "type": "function",
          "function": { "name": output_tool_name },
        },
        "parallel_tool_calls": false,
        "reasoning_effort": "none",
      },
      strict_tool_output_name = output_tool_name,
    )
  }
}
