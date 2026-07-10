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

class OpenAiSession {
  owner: OpenAiResponses,
  id: string,
  previous_response_id: string?,

  implements Session {
    function run<T>(self, request: Request<T>) -> T throws baml.errors.UnknownError {
      self.run_with_meta<T>(request).value
    }
    function run_with_meta<T>(self, request: Request<T>) -> Response<T> throws baml.errors.UnknownError {
      let provider = match (self.previous_response_id) {
        null => self.owner,
        let previous: string => OpenAiResponses {
          ...self.owner,
          extra_body: { "previous_response_id": previous, "store": true },
          store: true,
        },
      };
      let response = provider.generate<T>(request.for_provider(provider));
      self.previous_response_id = response.meta.request_id;
      response
    }
    function provider(self) -> Provider throws never { self.owner }
    function token(self) -> SessionToken throws never {
      SessionToken { provider: "openai-responses", id: self.previous_response_id ?? self.id }
    }
    function cleanup(self) -> null throws never { null }
  }

  implements ForkableSession {
    function fork(self) -> ForkableSession throws never {
      OpenAiSession {
        owner: self.owner,
        id: self.id + "-fork",
        previous_response_id: self.previous_response_id,
      }
    }
  }

  implements CompactableSession {
    function compact(self, policy: CompactionPolicy) -> CompactionResult throws never {
      CompactionResult { changed: false }
    }
  }
}

class CompletedLive {
  transcript: string,
  channel: Channel,

  implements Live {
    function events(self) -> LiveEventStream throws never {
      [TranscriptDelta { text: self.transcript }, LiveClosed {}]
    }
    function cancel_response(self) -> null throws never { null }
    function truncate_assistant_audio(self, played_ms: int) -> null throws never { null }
    function close(self) -> null throws never { self.channel.close() }
    function cleanup(self) -> null throws never { self.channel.close() }
  }
}

class OpenAiRealtime {
  model: string?,
  voice: string,
  api_key: string,

  implements Provider {
    function display_name(self) -> string throws never { "openai-realtime" }
    function render_shorthand(self) -> string throws never { "openai/" + (self.model ?? "gpt-5.6-luna") }
  }

  implements Realtime {
    function open_live<T>(self, request: Request<T>, channel: Channel) -> Live
        throws baml.errors.UnknownError {
      let ws = baml.ws.connect(
        "wss://api.openai.com/v1/realtime?model=" + (self.model ?? "gpt-5.6-luna"),
        { "Authorization": "Bearer " + self.api_key },
      ) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["realtime connect failed"] },
      };
      ws.send(baml.json.stringify({
        "type": "conversation.item.create",
        "item": {
          "type": "message", "role": "user",
          "content": [{ "type": "input_text", "text": baml.llm.prompt_to_text(request.prompt) }],
        },
      })) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["realtime send failed"] },
      };
      ws.send(baml.json.stringify({
        "type": "response.create", "response": { "output_modalities": ["text"] },
      })) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["realtime response.create failed"] },
      };
      let text = "";
      let done = false;
      while (!done) {
        let next = ws.next() catch (e) {
          _ => throw baml.errors.UnknownError { data: e, message: ["realtime receive failed"] },
        };
        match (next) {
          null => { done = true; },
          let raw: string => {
            channel.send(raw) catch (e) { _ => null };
            let event = baml.json.parse(raw) catch (e) {
              _ => throw baml.errors.UnknownError { data: raw, message: ["realtime event JSON failed"] },
            };
            let kind = baml.json.path_or(event, ".type", "");
            if (kind == "response.output_text.delta" || kind == "response.text.delta") {
              text = text + baml.json.path_or(event, ".delta", "");
            }
            if (kind == "response.done" || kind == "response.completed") { done = true; }
            if (kind == "error" || kind == "response.failed") {
              ws.close();
              throw baml.errors.UnknownError { data: raw, message: ["realtime error event"] };
            }
          },
        }
      }
      ws.close();
      CompletedLive { transcript: text, channel: channel }
    }
  }
}
