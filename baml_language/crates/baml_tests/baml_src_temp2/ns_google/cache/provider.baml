function _cache_messages_text(messages: ai.Messages) -> string throws never {
    messages
        .items()
        .map((message: ai.Message) -> string {
            let role = match (message.role()) {
                ai.MessageRole.System => "system",
                ai.MessageRole.User => "user",
                ai.MessageRole.Assistant => "assistant",
                ai.MessageRole.Tool => "tool",
            };
            role
                + ": "
                + message
                    .parts()
                    .map((part: ai.MessagePart) -> string {
                        part.text() ?? ""
                    })
                    .join("")
        })
        .join("\n")
}

// Gemini's generateContent API exposes a true remote CachedContent resource:
// it has a provider ID, TTL, model ownership, run-time reference, and DELETE.
class Gemini {
    model: string,
    api_key: string,
    base_url: string?,
    created_keys: string[],
    deleted_keys: string[],

    implements ai.Provider {
        function name(self) -> string throws never {
            "gemini-managed-cache"
        }
        function render_shorthand(self) -> string throws never {
            "google-ai/" + self.model
        }
    }

    function cache_exists(
        self,
        key: string,
    ) -> bool throws ai.Failure | baml.errors.UnknownError {
        let response = baml
            .http
            .send(
                baml.http.Request {
                    method: "GET",
                    url: (self.base_url ?? "https://generativelanguage.googleapis.com/v1beta")
                        + "/"
                        + key
                        + "?key="
                        + self.api_key,
                    headers: { "Content-Type": "application/json" },
                    body: "",
                },
            ) catch (e) {
            _ => {
                throw baml.errors.UnknownError.with_message<never>(e, "Gemini cache metadata lookup failed")
            },
        };
        // Gemini intentionally does not distinguish a missing cache from one this
        // credential cannot inspect: both may be 403 PERMISSION_DENIED with
        // "not found (or permission denied)". For an existence predicate, either
        // means the resource is not accessible to this configured provider.
        if (response.status_code == 403 || response.status_code == 404) {
            return false;
        }
        if (!response.ok()) {
            let body = response.text() catch (e) {
                _ => ""
            };
            throw ai.internal._classify_http("google", response.status_code, body);
        }
        true
    }
}

class ManagedCache {
    owner: Gemini,
    cache_key: string,
    deleted: bool,
    runs: int,
    cached_tokens: int,

    // function cleanup(self) -> void throws never {
    //   self.delete() catch (e) { _ => null };
    // }

    implements root.google.Cache {
        function key(self) -> string throws never {
            self.cache_key
        }

        function run<T>(
            self,
            task: ai.Task<T>,
        ) -> ai.ResponseWithMetadata<T> throws ai.Failure | baml.errors.UnknownError {
            if (self.deleted) {
                throw baml.errors.UnknownError.with_message<never>(self.cache_key, "cannot run a deleted Gemini cache");
            }
            //# Reference the remote cache instead of resending its contents
            let response = baml
                .http
                .send(
                    baml.http.Request {
                        method: "POST",
                        url: (self.owner.base_url ?? "https://generativelanguage.googleapis.com/v1beta")
                            + "/models/"
                            + self.owner.model
                            + ":generateContent?key="
                            + self.owner.api_key,
                        headers: { "Content-Type": "application/json" },
                        body: baml
                            .json
                            .stringify(
                                {
                                    "cachedContent": self.cache_key,
                                    "contents": task.prompt.messages().map((message: baml.llm.PromptMessage) -> json {
                                        {
                                            "role": ai.messages._prompt_role_or_user(message.role),
                                            "parts": [{ "text": message.content }],
                                        }
                                    }),
                                },
                            ),
                    },
                ) catch (e) {
                _ => {
                    throw baml.errors.UnknownError.with_message<never>(e, "Gemini cached generation failed")
                },
            };
            let body = response.text() catch (e) {
                _ => {
                    throw baml.errors.UnknownError.with_message<never>(e, "Gemini cached generation body failed")
                },
            };
            if (!response.ok()) {
                throw ai.internal._classify_http("google", response.status_code, body);
            }
            let raw = baml.json.parse(body) catch (e) {
                _ => {
                    throw baml.errors.UnknownError.with_message<never>(e, "Gemini cached generation JSON failed")
                },
            };
            let output = baml.json.path_or(raw, ".candidates[0].content.parts[0].text", "");
            let value = baml.sap.parse<T>(output) catch (e) {
                _ => {
                    throw baml.errors.UnknownError.with_message<never>(e, "Gemini cached generation typed parse failed")
                },
            };
            self.runs = self.runs + 1;
            ai.ResponseWithMetadata<T> {
                value: value,
                metadata: ai.ResponseMetadata {
                    provider: self.owner.name(),
                    model: self.owner.model,
                    request_id: baml.json.path_or<string?>(raw, ".responseId", null),
                    finish_reason: baml.json.path_or<string?>(raw, ".candidates[0].finishReason", null),
                    usage: ai.Usage {
                        input_tokens: baml.json.path_or(raw, ".usageMetadata.promptTokenCount", 0),
                        output_tokens: baml.json.path_or(raw, ".usageMetadata.candidatesTokenCount", 0),
                        cached_input_tokens: baml.json.path_or<int?>(raw, ".usageMetadata.cachedContentTokenCount", null),
                        reasoning_tokens: baml.json.path_or<int?>(raw, ".usageMetadata.thoughtsTokenCount", null),
                    },
                    attributes: {
                        "cache_key": self.cache_key,
                        "cache_runs": self.runs,
                        "cached_tokens_at_create": self.cached_tokens,
                    },
                    raw: raw,
                },
                conversation: null,
            }
        }

        function delete(self) -> null throws ai.Failure | baml.errors.UnknownError {
            if (self.deleted) {
                return null;
            }
            //# Delete the provider-owned cache resource
            let response = baml
                .http
                .send(
                    baml.http.Request {
                        method: "DELETE",
                        url: (self.owner.base_url ?? "https://generativelanguage.googleapis.com/v1beta")
                            + "/"
                            + self.cache_key
                            + "?key="
                            + self.owner.api_key,
                        headers: { "Content-Type": "application/json" },
                        body: "",
                    },
                ) catch (e) {
                _ => {
                    throw baml.errors.UnknownError.with_message<never>(e, "Gemini cache delete failed")
                },
            };
            let body = response.text() catch (e) {
                _ => ""
            };
            if (!response.ok() && response.status_code != 404) {
                throw ai.internal._classify_http("google", response.status_code, body);
            }
            self.deleted = true;
            self.owner.deleted_keys.push(self.cache_key);
            null
        }
    }

    implements ai.Resource {}
}

implements root.google.ManagedCacheProvider for Gemini {
  function create_cache(
    self,
    messages: ai.Messages,
    options: root.google.CacheOptions,
  ) -> root.google.Cache throws ai.Failure | baml.errors.UnknownError {
    let source = _cache_messages_text(messages);
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
      _ => throw baml.errors.UnknownError.with_message<never>(e, "Gemini cache create failed"),
    };
    let body = response.text() catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(e, "Gemini cache create body failed"),
    };
    if (!response.ok()) {
      throw ai.internal._classify_http("google", response.status_code, body);
    }
    let raw = baml.json.parse(body) catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(e, "Gemini cache create JSON failed"),
    };
    let key = baml.json.path_or(raw, ".name", "");
    if (key.length() == 0) {
      throw baml.errors.UnknownError.with_message<never>(raw, "Gemini cache create returned no resource name");
    }
    self.created_keys.push(key);
    ManagedCache {
      owner: self,
      cache_key: key,
      deleted: false,
      runs: 0,
      cached_tokens: baml.json.path_or(raw, ".usageMetadata.totalTokenCount", 0),
    }
  }
}
