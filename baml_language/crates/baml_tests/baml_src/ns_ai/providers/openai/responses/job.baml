class RemoteFailure {
  message: string,
  committed: CommitState,

  implements Failure {
    function kind(self) -> FailureKind throws never { FailureKind.Transport }
    function commit_state(self) -> CommitState throws never { self.committed }
  }
}

class OpenAiResponseJob<T> {
  owner: OpenAiResponses,
  id: string,

  implements Job<T> {
    function poll(self) -> Done<T> | Pending | Failed | Cancelled throws baml.errors.UnknownError {
      let response = baml.http.send(baml.http.Request {
        method: "GET",
        url: (self.owner.base_url ?? "https://api.openai.com/v1") + "/responses/" + self.id,
        headers: {
          "Authorization": "Bearer " + self.owner.api_key,
          "Content-Type": "application/json",
        },
        body: "",
      }) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["background poll failed"] },
      };
      let body = response.text() catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["background poll body failed"] },
      };
      let raw = baml.json.parse(body) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["background poll JSON failed"] },
      };
      let status = baml.json.path_or(raw, ".status", "");
      if (status == "cancelled") { return Cancelled {}; }
      if (status == "failed" || status == "incomplete") {
        return Failed {
          error: RemoteFailure { message: status, committed: CommitState.Committed },
        };
      }
      if (status != "completed") {
        return Pending { retry_after: baml.time.Duration.from_seconds(1) };
      }
      let primitive = _configured_primitive(
        self.owner.render_shorthand(), self.owner.model, self.owner.api_key,
        self.owner.base_url, self.owner.extra_headers, self.owner.extra_body,
      );
      let value: T = primitive.parse<T>(body) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["background result parse failed"] },
      };
      Done<T> {
        value: value,
        meta: Meta {
          provider: "openai-responses", model: self.owner.model, request_id: self.id,
          finish_reason: status,
          usage: Usage {
            input_tokens: baml.json.path_or(raw, ".usage.input_tokens", 0),
            output_tokens: baml.json.path_or(raw, ".usage.output_tokens", 0),
          },
          attributes: {}, raw: raw,
        },
      }
    }

    function cancel(self) -> null throws baml.errors.UnknownError {
      let response = baml.http.send(baml.http.Request {
        method: "POST",
        url: (self.owner.base_url ?? "https://api.openai.com/v1") + "/responses/" + self.id + "/cancel",
        headers: {
          "Authorization": "Bearer " + self.owner.api_key,
          "Content-Type": "application/json",
        },
        body: "{}",
      }) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["background cancel failed"] },
      };
      let _ = response.text() catch (e) { _ => "" };
      null
    }

    function token(self) -> JobToken throws never {
      JobToken { provider: "openai-responses", id: self.id }
    }

    function cleanup(self) -> null throws never { null }
  }
}
