// 31 — explicit provider-managed caches are resources because they have a
// billable lifecycle. Automatic prefix reuse is merely response metadata.

class s31_ExpiredCache {
  implements baml.errors.Failure {
    function is_retryable(self) -> bool { false }
    function is_effectful(self) -> bool { false }
    function is_policy_refusal(self) -> bool { false }
    function is_resumable(self) -> bool { false }
    function is_unsupported(self) -> bool { false }
  }
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { false }
    function is_rate_limit(self) -> bool { false }
    function is_parse_error(self) -> bool { false }
  }
}

class s31_Ledger {
  active_caches: int,
  cold_input_tokens: int,
  cached_input_tokens: int,
  last_prompt: string,
}

class s31_ManagedResource {
  owner: root.ai.Provider,
  reply: string,
  prefix: string,
  ledger: s31_Ledger,
  active: bool,

  implements root.ai.Cache {
    function run<T>(self, request: root.ai.Request<T>) -> T
        throws baml.errors.CallError | baml.errors.UnknownError {
      if (!self.active) { throw s31_ExpiredCache {}; }
      let rebound = request.for_provider(self.owner);
      self.ledger.last_prompt = self.prefix + "\n" + baml.llm.prompt_to_text(rebound.prompt);
      self.ledger.cached_input_tokens = self.ledger.cached_input_tokens + 10;
      baml.json.from_string<T>(self.reply) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["s31 cached parse failed"] },
      }
    }

    function delete(self) -> null throws baml.errors.UnknownError {
      if (self.active) {
        self.active = false;
        self.ledger.active_caches = self.ledger.active_caches - 1;
      }
      null
    }

    function cleanup(self) -> null throws never {
      if (self.active) {
        self.active = false;
        self.ledger.active_caches = self.ledger.active_caches - 1;
      }
      null
    }
  }
}

class s31_ManagedFake {
  reply: string,
  ledger: s31_Ledger,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s31-managed" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      self.ledger.cold_input_tokens = self.ledger.cold_input_tokens + 1000;
      let value = baml.json.from_string<T>(self.reply) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["s31 cold parse failed"] },
      };
      root.ai.Response<T> {
        value: value,
        meta: root.ai.Meta {
          provider: self.display_name(), model: null, request_id: null,
          finish_reason: "stop",
          usage: root.ai.Usage { input_tokens: 1000, output_tokens: 5 },
          attributes: {}, raw: null,
        },
      }
    }
  }

  implements root.ai.ManagedCache {
    function create_cache(
      self,
      content: root.ai.ChatMessage[],
      options: root.ai.CacheOptions,
    ) -> root.ai.Cache throws baml.errors.UnknownError {
      let prefix = content.map((message) -> {
        message.parts.map((part) -> { part.text ?? "" }).join("")
      }).join("\n");
      self.ledger.active_caches = self.ledger.active_caches + 1;
      s31_ManagedResource {
        owner: self, reply: self.reply, prefix: prefix,
        ledger: self.ledger, active: true,
      }
    }
  }
}

function s31_fresh_fake(reply: string = `"cached answer"`) -> s31_ManagedFake throws never {
  s31_ManagedFake {
    reply: reply,
    ledger: s31_Ledger {
      active_caches: 0, cold_input_tokens: 0,
      cached_input_tokens: 0, last_prompt: "",
    },
  }
}

function s31_supports_managed_cache(provider: root.ai.Provider) -> bool throws never {
  match (provider) {
    let managed: root.ai.ManagedCache => true,
    _ => false,
  }
}
