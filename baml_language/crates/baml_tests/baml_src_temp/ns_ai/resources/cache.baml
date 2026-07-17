class CacheOptions {
  ttl: baml.time.Duration,
}

interface Cache requires Resource {
  function key(self) -> string throws never
  function run<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError
  function delete(self) -> null throws baml.errors.UnknownError
}

interface ManagedCacheProvider requires Provider {
  function create_cache(self, messages: Messages, options: CacheOptions) -> Cache
    throws baml.errors.UnknownError
}

class FakeCache {
  cache_key: string,
  deleted: bool,
  runs: int,

  function cleanup(self) -> void throws never { self.delete(); }

  implements Cache {
    function key(self) -> string throws never { self.cache_key }
    function run<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
      self.runs = self.runs + 1;
      match (task.provider) {
        let provider: GenerationProvider => provider.generate<T>(task) catch (e) {
          let unknown: baml.errors.UnknownError => throw unknown,
          _ => throw baml.errors.UnknownError {
            data: e,
            message: ["fake cache generation failed"],
          },
        },
        _ => throw baml.errors.UnknownError {
          data: task.provider_name(),
          message: ["fake cache task provider cannot generate"],
        },
      }
    }
    function delete(self) -> null throws never { self.deleted = true; null }
  }
  implements Resource {}
}

class FakeCacheProvider {
  implements Provider {
    function name(self) -> string throws never { "fake-cache" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }
  implements ManagedCacheProvider {
    function create_cache(self, messages: Messages, options: CacheOptions) -> Cache throws never {
      FakeCache { cache_key: "fake-cache-key", deleted: false, runs: 0 }
    }
  }
}

function cache_messages_text(messages: Messages) -> string throws never {
  messages.items().map((message: Message) -> string {
    let role = match (message.role()) {
      MessageRole.System => "system",
      MessageRole.User => "user",
      MessageRole.Assistant => "assistant",
      MessageRole.Tool => "tool",
    };
    role + ": " + message.parts().map((part: MessagePart) -> string {
      part.text() ?? ""
    }).join("")
  }).join("\n")
}

// Gemini's generateContent API exposes a true remote CachedContent resource:
// it has a provider ID, TTL, model ownership, run-time reference, and DELETE.
class GeminiManagedCacheProvider {
  model: string,
  api_key: string,
  base_url: string?,
  created_keys: string[],
  deleted_keys: string[],
  implements Provider {
    function name(self) -> string throws never { "gemini-managed-cache" }
    function render_shorthand(self) -> string throws never { "google-ai/" + self.model }
  }

  function cache_exists(self, key: string) -> bool throws baml.errors.UnknownError {
    let response = baml.http.send(baml.http.Request {
      method: "GET",
      url: (self.base_url ?? "https://generativelanguage.googleapis.com/v1beta")
        + "/" + key + "?key=" + self.api_key,
      headers: { "Content-Type": "application/json" },
      body: "",
    }) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["Gemini cache metadata lookup failed"],
      },
    };
    // Gemini intentionally does not distinguish a missing cache from one this
    // credential cannot inspect: both may be 403 PERMISSION_DENIED with
    // "not found (or permission denied)". For an existence predicate, either
    // means the resource is not accessible to this configured provider.
    if (response.status_code == 403 || response.status_code == 404) { return false; }
    if (!response.ok()) {
      let body = response.text() catch (e) { _ => "" };
      throw baml.errors.UnknownError {
        data: body,
        message: ["Gemini cache metadata HTTP " + response.status_code.to_string()],
      };
    }
    true
  }
}

class GeminiManagedCache {
  owner: GeminiManagedCacheProvider,
  cache_key: string,
  deleted: bool,
  runs: int,
  cached_tokens: int,

  // Direct class method: cleanup is compiler magic only in this position.
  // An implementation written solely inside `implements Resource` is ordinary
  // interface dispatch and is not registered as a GC finalizer.
  function cleanup(self) -> void throws never {
    self.delete() catch (e) { _ => null };
  }

  implements Cache {
    function key(self) -> string throws never { self.cache_key }

    function run<T>(self, task: Task<T>) -> Response<T>
        throws baml.errors.UnknownError {
      if (self.deleted) {
        throw baml.errors.UnknownError {
          data: self.cache_key,
          message: ["cannot run a deleted Gemini cache"],
        };
      }
      //# Reference the remote cache instead of resending its contents
      let response = baml.http.send(baml.http.Request {
        method: "POST",
        url: (self.owner.base_url ?? "https://generativelanguage.googleapis.com/v1beta")
          + "/models/" + self.owner.model + ":generateContent?key=" + self.owner.api_key,
        headers: { "Content-Type": "application/json" },
        body: baml.json.stringify({
          "cachedContent": self.cache_key,
          "contents": [{
            "role": "user",
            "parts": [{ "text": baml.llm.prompt_to_text(task.prompt) }],
          }],
        }),
      }) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["Gemini cached generation failed"],
        },
      };
      let body = response.text() catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["Gemini cached generation body failed"],
        },
      };
      if (!response.ok()) {
        throw baml.errors.UnknownError {
          data: body,
          message: ["Gemini cached generation HTTP " + response.status_code.to_string()],
        };
      }
      let raw = baml.json.parse(body) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["Gemini cached generation JSON failed"],
        },
      };
      let output = baml.json.path_or(raw, ".candidates[0].content.parts[0].text", "");
      let value = baml.sap.parse<T>(output) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["Gemini cached generation typed parse failed"],
        },
      };
      self.runs = self.runs + 1;
      Response<T> {
        value: value,
        meta: Meta {
          provider: self.owner.name(),
          model: self.owner.model,
          request_id: baml.json.path_or<string?>(raw, ".responseId", null),
          finish_reason: baml.json.path_or<string?>(raw, ".candidates[0].finishReason", null),
          usage: Usage {
            input_tokens: baml.json.path_or(raw, ".usageMetadata.promptTokenCount", 0),
            output_tokens: baml.json.path_or(raw, ".usageMetadata.candidatesTokenCount", 0),
            cached_input_tokens: baml.json.path_or<int?>(raw, ".usageMetadata.cachedContentTokenCount", null),
            reasoning_tokens: baml.json.path_or<int?>(raw, ".usageMetadata.thoughtsTokenCount", null),
            cost_usd: null,
          },
          attributes: {
            "cache_key": self.cache_key,
            "cache_runs": self.runs,
            "cached_tokens_at_create": self.cached_tokens,
          },
          raw: raw,
        },
        transcript: null,
      }
    }

    function delete(self) -> null throws baml.errors.UnknownError {
      if (self.deleted) { return null; }
      //# Delete the provider-owned cache resource
      let response = baml.http.send(baml.http.Request {
        method: "DELETE",
        url: (self.owner.base_url ?? "https://generativelanguage.googleapis.com/v1beta")
          + "/" + self.cache_key + "?key=" + self.owner.api_key,
        headers: { "Content-Type": "application/json" },
        body: "",
      }) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["Gemini cache delete failed"],
        },
      };
      let body = response.text() catch (e) { _ => "" };
      if (!response.ok() && response.status_code != 404) {
        throw baml.errors.UnknownError {
          data: body,
          message: ["Gemini cache delete HTTP " + response.status_code.to_string()],
        };
      }
      self.deleted = true;
      self.owner.deleted_keys.push(self.cache_key);
      null
    }
  }

  implements Resource {}
}

implements ManagedCacheProvider for GeminiManagedCacheProvider {
  function create_cache(self, messages: Messages, options: CacheOptions) -> Cache
      throws baml.errors.UnknownError {
    let source = cache_messages_text(messages);
    //# Create a remote cache with explicit model ownership and TTL
    let response = baml.http.send(baml.http.Request {
      method: "POST",
      url: (self.base_url ?? "https://generativelanguage.googleapis.com/v1beta")
        + "/cachedContents?key=" + self.api_key,
      headers: { "Content-Type": "application/json" },
      body: baml.json.stringify({
        "model": "models/" + self.model,
        "contents": [{
          "role": "user",
          "parts": [{ "text": source }],
        }],
        "ttl": options.ttl.to_seconds().to_string() + "s",
      }),
    }) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["Gemini cache create failed"],
      },
    };
    let body = response.text() catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["Gemini cache create body failed"],
      },
    };
    if (!response.ok()) {
      throw baml.errors.UnknownError {
        data: body,
        message: ["Gemini cache create HTTP " + response.status_code.to_string()],
      };
    }
    let raw = baml.json.parse(body) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["Gemini cache create JSON failed"],
      },
    };
    let key = baml.json.path_or(raw, ".name", "");
    if (key.length() == 0) {
      throw baml.errors.UnknownError {
        data: raw,
        message: ["Gemini cache create returned no resource name"],
      };
    }
    self.created_keys.push(key);
    GeminiManagedCache {
      owner: self,
      cache_key: key,
      deleted: false,
      runs: 0,
      cached_tokens: baml.json.path_or(raw, ".usageMetadata.totalTokenCount", 0),
    }
  }
}
