// OpenAI Responses background mode is a remote Job<T>: submission returns an
// opaque response ID, polling may happen in another process, and only the token
// plus a separately configured provider are needed to resume it.
class Responses {
    model: string,
    api_key: string,
    base_url: string?,

    implements root.ai.Provider {
        function name(self) -> string throws never {
            "openai-background"
        }
        function render_shorthand(self) -> string throws never {
            "openai-responses/" + self.model
        }
    }
}

class BackgroundJob<T> {
    owner: Responses,
    id: string,
    last_status: root.ai.JobStatus,
    polls: int,

    // function cleanup(self) -> void throws never {
    //   if (self.last_status == root.ai.JobStatus.Pending) {
    //     self.cancel() catch (e) { _ => null };
    //   }
    // }

    implements root.ai.Job<T> {
        function status(self) -> root.ai.JobStatus throws never {
            self.last_status
        }

        function poll(self) -> root.ai.ResponseWithMetadata<T>? throws baml.errors.UnknownError {
            if (self.last_status == root.ai.JobStatus.Cancelled) {
                return null;
            }
            self.polls = self.polls + 1;
            //# Fetch the durable provider response by opaque ID
            let response = baml
                .http
                .send(
                    baml.http.Request {
                        method: "GET",
                        url: (self.owner.base_url ?? "https://api.openai.com/v1")
                            + "/responses/"
                            + self.id,
                        headers: {
                            "Authorization": "Bearer " + self.owner.api_key,
                            "Content-Type": "application/json",
                        },
                        body: "",
                    },
                ) catch (e) {
                _ => {
                    throw baml.errors.UnknownError {
          data: e,
          message: ["background poll failed"],
        }
                },
            };
            let body = response.text() catch (e) {
                _ => {
                    throw baml.errors.UnknownError {
          data: e,
          message: ["background poll body failed"],
        }
                },
            };
            if (!response.ok()) {
                throw baml.errors.UnknownError {
          data: body,
          message: ["background poll HTTP " + response.status_code.to_string()],
        };
            }
            let raw = baml.json.parse(body) catch (e) {
                _ => {
                    throw baml.errors.UnknownError {
          data: e,
          message: ["background poll JSON failed"],
        }
                },
            };
            let remote_status = baml.json.path_or(raw, ".status", "");
            if (remote_status == "cancelled") {
                self.last_status = root.ai.JobStatus.Cancelled;
                return null;
            }
            if (remote_status == "failed" || remote_status == "incomplete") {
                self.last_status = root.ai.JobStatus.Failed;
                throw baml.errors.UnknownError {
          data: raw,
          message: ["background response " + remote_status],
        };
            }
            if (remote_status != "completed") {
                self.last_status = root.ai.JobStatus.Pending;
                return null;
            }
            //## Parse the same T that was captured by Job<T> at submission
            let primitive = _configured_primitive(
                self.owner.render_shorthand(),
                self.owner.model,
                self.owner.api_key,
                self.owner.base_url,
                null,
                null,
            );
            let value = primitive.parse<T>(body) catch (e) {
                _ => {
                    throw baml.errors.UnknownError {
          data: e,
          message: ["background typed result parse failed"],
        }
                },
            };
            self.last_status = root.ai.JobStatus.Complete;
            root.ai.ResponseWithMetadata<T> {
                value: value,
                meta: root.ai.ResponseMetadata {
                    provider: self.owner.name(),
                    model: baml.json.path_or<string?>(raw, ".model", self.owner.model),
                    request_id: self.id,
                    finish_reason: remote_status,
                    usage: root.ai.Usage {
                        input_tokens: baml.json.path_or(raw, ".usage.input_tokens", 0),
                        output_tokens: baml.json.path_or(raw, ".usage.output_tokens", 0),
                        cached_input_tokens: baml
                            .json
                            .path_or<int?>(raw, ".usage.input_tokens_details.cached_tokens", null),
                        reasoning_tokens: baml
                            .json
                            .path_or<int?>(
                                raw,
                                ".usage.output_tokens_details.reasoning_tokens",
                                null,
                            ),
                        cost_usd: null,
                    },
                    attributes: { "polls": self.polls, "background": true },
                    raw: raw,
                },
                conversation: null,
            }
        }

        function cancel(self) -> null throws baml.errors.UnknownError {
            if (self.last_status == root.ai.JobStatus.Cancelled) {
                return null;
            }
            //# Cancel remote work without discarding its resume coordinates
            let response = baml
                .http
                .send(
                    baml.http.Request {
                        method: "POST",
                        url: (self.owner.base_url ?? "https://api.openai.com/v1")
                            + "/responses/"
                            + self.id
                            + "/cancel",
                        headers: {
                            "Authorization": "Bearer " + self.owner.api_key,
                            "Content-Type": "application/json",
                        },
                        body: "{}",
                    },
                ) catch (e) {
                _ => {
                    throw baml.errors.UnknownError {
          data: e,
          message: ["background cancel failed"],
        }
                },
            };
            let body = response.text() catch (e) {
                _ => ""
            };
            if (!response.ok() && self.last_status != root.ai.JobStatus.Complete) {
                throw baml.errors.UnknownError {
          data: body,
          message: ["background cancel HTTP " + response.status_code.to_string()],
        };
            }
            self.last_status = root.ai.JobStatus.Cancelled;
            null
        }

        function token(self) -> root.ai.JobToken throws never {
            root.ai.JobToken { provider: self.owner.name(), id: self.id, sealed: null }
        }
    }

    implements root.ai.Resource {}
}

implements root.ai.BackgroundProvider for Responses {
  function submit<T>(
    self,
    task: root.ai.Task<T>,
    options: root.ai.BackgroundOptions,
  ) -> root.ai.Job<T> throws baml.errors.UnknownError {
    let primitive = _configured_primitive(
      self.render_shorthand(),
      self.model,
      self.api_key,
      self.base_url,
      null,
      {
        "background": true,
        "store": true,
      },
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
      let key: string => {
        let _ = headers.set("Idempotency-Key", key);
      },
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
    BackgroundJob<T> {
      owner: self,
      id: id,
      last_status: if (baml.json.path_or(raw, ".status", "") == "completed") {
        root.ai.JobStatus.Complete
      } else {
        root.ai.JobStatus.Pending
      },
      polls: 0,
    }
  }

  function resume_job<T>(
    self,
    token: root.ai.JobToken,
  ) -> root.ai.Job<T> throws baml.errors.UnknownError {
    if (token.provider != self.name()) {
      throw baml.errors.UnknownError {
        data: token,
        message: ["background job token provider mismatch"],
      };
    }
    BackgroundJob<T> {
      owner: self,
      id: token.id,
      last_status: root.ai.JobStatus.Pending,
      polls: 0,
    }
  }
}
