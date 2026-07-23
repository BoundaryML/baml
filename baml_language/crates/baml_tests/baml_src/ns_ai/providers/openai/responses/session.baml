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
      let response = provider.generate<T>(request.for_provider(provider)) catch (e) {
        let unknown: baml.errors.UnknownError => throw unknown,
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["session generation failed"],
        },
      };
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
