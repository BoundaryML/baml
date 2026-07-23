implements Realtime for OpenAiRealtime {
  function open_live<T>(self, request: Request<T>, channel: Channel) -> Live
      throws baml.errors.UnknownError {
    let ws = baml.ws.connect(
      "wss://api.openai.com/v1/realtime?model=" + (self.model ?? "gpt-realtime"),
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
