// 06 — a Generate provider may return media instead of prose. The semantic
// capability stays the same; only this provider's private wire codec differs.
class s06_Media {
  mime: string,
  data: string?,
  url: string?,
}

class s06_ImageDatumWire {
  b64_json: string?,
  revised_prompt: string?,
}

class s06_ImageWire {
  data: s06_ImageDatumWire[],
}

function s06_messages_text(messages: root.ai.ChatMessage[]) -> string throws never {
  let out = "";
  for (let message in messages) {
    for (let part in message.parts) {
      out = out + (part.text ?? "");
    }
  }
  out
}

function s06_as<S, T>(value: S) -> T throws baml.errors.UnknownError {
  let encoded = baml.json.to_string<S>(value) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: ["media encode failed"] },
  };
  baml.sap.parse<T>(encoded) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: ["media did not match task output"] },
  }
}

class s06_GptImage {
  model: string,
  api_key: string,
  size: string,

  implements root.ai.Provider {}

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      let body: map<string, baml.json.json> = {
        "model": self.model,
        "prompt": s06_messages_text(request.messages()),
        "size": self.size,
      };
      log.info(baml.json.stringify({
        "event": "llm.image.request",
        "provider": "openai-images",
        "model": self.model,
        "body": baml.json.stringify(body),
      }));
      let response = baml.http.send(baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/images/generations",
        headers: {
          "authorization": "Bearer " + self.api_key,
          "content-type": "application/json",
        },
        body: baml.json.stringify(body),
      }) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["image request failed"] },
      };
      let text = response.text() catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["image response read failed"] },
      };
      log.info(baml.json.stringify({
        "event": "llm.image.response",
        "provider": "openai-images",
        "model": self.model,
        "status_code": response.status_code,
        "body": text,
      }));
      if (!response.ok()) {
        throw baml.errors.UnknownError { data: text, message: ["image provider refused request"] };
      }
      let raw = baml.json.parse(text) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["image response parse failed"] },
      };
      let wire: s06_ImageWire = baml.json.from_json<s06_ImageWire>(raw) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["image response parse failed"] },
      };
      let first = wire.data.at(0);
      match (first) {
        let image: s06_ImageDatumWire => {
          let value = s06_Media {
            mime: "image/png",
            data: image.b64_json,
            url: null,
          };
          root.ai.Response<T> {
            value: s06_as<s06_Media, T>(value),
            meta: root.ai.Meta {
              provider: "openai-images",
              model: self.model,
              request_id: null,
              finish_reason: "stop",
              usage: null,
              attributes: { "revised_prompt": image.revised_prompt },
              raw: raw,
            },
          }
        },
        _ => throw baml.errors.UnknownError { data: text, message: ["image response was empty"] },
      }
    }
  }
}
