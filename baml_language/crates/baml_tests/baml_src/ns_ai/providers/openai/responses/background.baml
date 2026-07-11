implements Background for OpenAiResponses {
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
