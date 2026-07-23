class BackgroundOptions {
  idempotency_key: string?,
}

class JobToken {
  provider: string,
  id: string,
  sealed: string?,
}

enum JobStatus {
  Pending
  Complete
  Failed
  Cancelled
}

interface Job<T> requires Resource {
  function status(self) -> JobStatus throws never
  function poll(self) -> Response<T>? throws baml.errors.UnknownError
  function cancel(self) -> null throws baml.errors.UnknownError
  function token(self) -> JobToken throws never
}

interface BackgroundProvider requires Provider {
  function submit<T>(self, task: Task<T>, options: BackgroundOptions) -> Job<T>
    throws baml.errors.UnknownError
  function resume_job<T>(self, token: JobToken) -> Job<T>
    throws baml.errors.UnknownError
}

class FakeJob<T> {
  job_token: JobToken,
  response: Response<T>,
  cancelled: bool,

  // function cleanup(self) -> void throws never { self.cancel(); }

  implements Job<T> {
    function status(self) -> JobStatus throws never {
      if (self.cancelled) { JobStatus.Cancelled } else { JobStatus.Complete }
    }
    function poll(self) -> Response<T>? throws never {
      if (self.cancelled) { null } else { self.response }
    }
    function cancel(self) -> null throws never { self.cancelled = true; null }
    function token(self) -> JobToken throws never { self.job_token }
  }
  implements Resource {}
}

class FakeBackgroundProvider {
  inner: FakeProvider,
}

implements Provider for FakeBackgroundProvider {
  function name(self) -> string throws never { "fake-background" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements BackgroundProvider for FakeBackgroundProvider {
  function submit<T>(self, task: Task<T>, options: BackgroundOptions) -> Job<T>
      throws baml.errors.UnknownError {
    FakeJob<T> {
      job_token: JobToken {
        provider: self.name(),
        id: options.idempotency_key ?? "fake-job",
        sealed: self.inner.output,
      },
      response: self.inner.generate<T>(task.with_provider(self.inner)) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["fake background generation failed"],
        },
      },
      cancelled: false,
    }
  }

  function resume_job<T>(self, token: JobToken) -> Job<T> throws baml.errors.UnknownError {
    let task = root.ai.task<T>(self.inner, (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.assemble_prompt_ast(["resume"], [])
    });
    self.submit<T>(task, BackgroundOptions { idempotency_key: token.id })
  }
}

// OpenAI Responses background mode is a remote Job<T>: submission returns an
// opaque response ID, polling may happen in another process, and only the token
// plus a separately configured provider are needed to resume it.
class OpenAiBackground {
  model: string,
  api_key: string,
  base_url: string?,
  implements Provider {
    function name(self) -> string throws never { "openai-background" }
    function render_shorthand(self) -> string throws never {
      "openai-responses/" + self.model
    }
  }
}

class OpenAiBackgroundJob<T> {
  owner: OpenAiBackground,
  id: string,
  last_status: JobStatus,
  polls: int,

  // function cleanup(self) -> void throws never {
  //   if (self.last_status == JobStatus.Pending) {
  //     self.cancel() catch (e) { _ => null };
  //   }
  // }

  implements Job<T> {
    function status(self) -> JobStatus throws never { self.last_status }

    function poll(self) -> Response<T>? throws baml.errors.UnknownError {
      if (self.last_status == JobStatus.Cancelled) { return null; }
      self.polls = self.polls + 1;
      //# Fetch the durable provider response by opaque ID
      let response = baml.http.send(baml.http.Request {
        method: "GET",
        url: (self.owner.base_url ?? "https://api.openai.com/v1") + "/responses/" + self.id,
        headers: {
          "Authorization": "Bearer " + self.owner.api_key,
          "Content-Type": "application/json",
        },
        body: "",
      }) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["background poll failed"],
        },
      };
      let body = response.text() catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["background poll body failed"],
        },
      };
      if (!response.ok()) {
        throw baml.errors.UnknownError {
          data: body,
          message: ["background poll HTTP " + response.status_code.to_string()],
        };
      }
      let raw = baml.json.parse(body) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["background poll JSON failed"],
        },
      };
      let remote_status = baml.json.path_or(raw, ".status", "");
      if (remote_status == "cancelled") {
        self.last_status = JobStatus.Cancelled;
        return null;
      }
      if (remote_status == "failed" || remote_status == "incomplete") {
        self.last_status = JobStatus.Failed;
        throw baml.errors.UnknownError {
          data: raw,
          message: ["background response " + remote_status],
        };
      }
      if (remote_status != "completed") {
        self.last_status = JobStatus.Pending;
        return null;
      }
      //## Parse the same T that was captured by Job<T> at submission
      let primitive = configured_primitive(
        self.owner.render_shorthand(),
        self.owner.model,
        self.owner.api_key,
        self.owner.base_url,
        null,
        null,
      );
      let value = primitive.parse<T>(body) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["background typed result parse failed"],
        },
      };
      self.last_status = JobStatus.Complete;
      Response<T> {
        value: value,
        meta: Meta {
          provider: self.owner.name(),
          model: baml.json.path_or<string?>(raw, ".model", self.owner.model),
          request_id: self.id,
          finish_reason: remote_status,
          usage: Usage {
            input_tokens: baml.json.path_or(raw, ".usage.input_tokens", 0),
            output_tokens: baml.json.path_or(raw, ".usage.output_tokens", 0),
            cached_input_tokens: baml.json.path_or<int?>(raw, ".usage.input_tokens_details.cached_tokens", null),
            reasoning_tokens: baml.json.path_or<int?>(raw, ".usage.output_tokens_details.reasoning_tokens", null),
            cost_usd: null,
          },
          attributes: { "polls": self.polls, "background": true },
          raw: raw,
        },
        conversation: null,
      }
    }

    function cancel(self) -> null throws baml.errors.UnknownError {
      if (self.last_status == JobStatus.Cancelled) { return null; }
      //# Cancel remote work without discarding its resume coordinates
      let response = baml.http.send(baml.http.Request {
        method: "POST",
        url: (self.owner.base_url ?? "https://api.openai.com/v1") + "/responses/" + self.id + "/cancel",
        headers: {
          "Authorization": "Bearer " + self.owner.api_key,
          "Content-Type": "application/json",
        },
        body: "{}",
      }) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["background cancel failed"],
        },
      };
      let body = response.text() catch (e) { _ => "" };
      if (!response.ok() && self.last_status != JobStatus.Complete) {
        throw baml.errors.UnknownError {
          data: body,
          message: ["background cancel HTTP " + response.status_code.to_string()],
        };
      }
      self.last_status = JobStatus.Cancelled;
      null
    }

    function token(self) -> JobToken throws never {
      JobToken { provider: self.owner.name(), id: self.id, sealed: null }
    }
  }

  implements Resource {}
}

implements BackgroundProvider for OpenAiBackground {
  function submit<T>(self, task: Task<T>, options: BackgroundOptions) -> Job<T>
      throws baml.errors.UnknownError {
    let primitive = configured_primitive(
      self.render_shorthand(),
      self.model,
      self.api_key,
      self.base_url,
      null,
      { "background": true, "store": true },
    );
    let specialized = primitive.specialize_prompt(task.prompt) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["background prompt specialization failed"],
      },
    };
    let built = primitive.build_request(specialized, task.output_type()) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["background request build failed"],
      },
    };
    let headers = built.headers;
    match (options.idempotency_key) {
      let key: string => { let _ = headers.set("Idempotency-Key", key); },
      null => {},
    }
    //# Submit durable remote work without waiting for T
    let response = baml.http.send(baml.http.Request {
      method: built.method,
      url: built.url,
      headers: headers,
      body: built.body,
    }) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["background submit failed"],
      },
    };
    let body = response.text() catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["background submit body failed"],
      },
    };
    if (!response.ok()) {
      throw baml.errors.UnknownError {
        data: body,
        message: ["background submit HTTP " + response.status_code.to_string()],
      };
    }
    let raw = baml.json.parse(body) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["background submit JSON failed"],
      },
    };
    let id = baml.json.path_or(raw, ".id", "");
    if (id.length() == 0) {
      throw baml.errors.UnknownError {
        data: raw,
        message: ["background submit returned no response ID"],
      };
    }
    OpenAiBackgroundJob<T> {
      owner: self,
      id: id,
      last_status: if (baml.json.path_or(raw, ".status", "") == "completed") {
        JobStatus.Complete
      } else {
        JobStatus.Pending
      },
      polls: 0,
    }
  }

  function resume_job<T>(self, token: JobToken) -> Job<T>
      throws baml.errors.UnknownError {
    if (token.provider != self.name()) {
      throw baml.errors.UnknownError {
        data: token,
        message: ["background job token provider mismatch"],
      };
    }
    OpenAiBackgroundJob<T> {
      owner: self,
      id: token.id,
      last_status: JobStatus.Pending,
      polls: 0,
    }
  }
}
