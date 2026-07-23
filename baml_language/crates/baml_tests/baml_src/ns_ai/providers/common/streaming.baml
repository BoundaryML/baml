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
  let cache = baml.llm.__new_stream_cache<TPartial, T>();
  baml.llm.Stream<TPartial, T> {
    _client: primitive,
    _acc: accumulator,
    _sse: sse,
    _cache: cache,
  }
}
