function primitive_stream<TPartial, T>(
  primitive: baml.llm.PrimitiveClient,
  task: Task<T>,
  provider_name: string,
) -> baml.llm.Stream<TPartial, T> throws baml.errors.UnknownError {
  let specialized = primitive.specialize_prompt(task.prompt) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " stream prompt failed"] },
  };
  let request = primitive.build_request_stream(specialized, task.output_type()) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " stream request failed"] },
  };
  log.info(baml.json.stringify({
    "event": "llm.stream.request",
    "provider": provider_name,
    "method": request.method,
    "url": request.url,
    // Prompts and encoded media must not leak into routine integration logs.
    "request_bytes": request.body.length(),
  }));
  let sse = baml.http.fetch_sse(request) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " stream open failed"] },
  };
  let accumulator = primitive.new_stream_accumulator() catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: [provider_name + " stream accumulator failed"] },
  };
  baml.llm.Stream<TPartial, T> {
    _client: primitive,
    _acc: accumulator,
    _sse: sse,
    _cache: baml.llm.__new_stream_cache<TPartial, T>(),
  }
}
