implements root.ai.jobs.BackgroundProvider for Responses {
  function submit<T>(
    self,
    task: root.ai.Task<T>,
    options: root.ai.jobs.BackgroundOptions,
  ) -> root.ai.jobs.Job<T> throws root.ai.Failure | baml.errors.UnknownError {
    let request = root.openai.internal._responses_built_request(self, task, true);
    let headers = request.headers;
    match (options.idempotency_key) {
      let key: string => {
        let _ = headers.set("Idempotency-Key", key);
      },
      null => {},
    }
    //# Submit durable remote work without waiting for T
    let response = baml.http.send(baml.http.Request {
      method: request.method,
      url: request.url,
      headers: headers,
      body: request.body,
    }) catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(
        e,
        "background submit failed",
      ),
    };
    let body = response.text() catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(
        e,
        "background submit body failed",
      ),
    };
    if (!response.ok()) {
      throw root.ai.internal.classify_http(
        "openai",
        response.status_code,
        body,
      );
    }
    let raw = baml.json.parse(body) catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(
        e,
        "background submit JSON failed",
      ),
    };
    let id = baml.json.path_or(raw, ".id", "");
    if (id.length() == 0) {
      throw baml.errors.UnknownError.with_message<never>(
        raw,
        "background submit returned no response ID",
      );
    }
    root.openai.internal.BackgroundJob<T> {
      owner: self,
      id: id,
      last_status: if (baml.json.path_or(raw, ".status", "") == "completed") {
        root.ai.jobs.JobStatus.Complete
      } else {
        root.ai.jobs.JobStatus.Pending
      },
      polls: 0,
    }
  }

  function resume_job<T>(
    self,
    token: root.ai.jobs.JobToken,
  ) -> root.ai.jobs.Job<T> throws baml.errors.UnknownError {
    if (token.provider != self.name()) {
      throw baml.errors.UnknownError.with_message<never>(
        token,
        "background job token provider mismatch",
      );
    }
    root.openai.internal.BackgroundJob<T> {
      owner: self,
      id: token.id,
      last_status: root.ai.jobs.JobStatus.Pending,
      polls: 0,
    }
  }
}
