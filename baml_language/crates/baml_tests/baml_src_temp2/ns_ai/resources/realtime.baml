interface Channel {
  function send(self, frame: string) -> null throws baml.errors.Io
  function close(self) -> null throws never
}

class RecordingChannel {
  frames: string[],
  implements Channel {
    function send(self, frame: string) -> null throws never { self.frames.push(frame); null }
    function close(self) -> null throws never { null }
  }
}

interface LiveEvent {
  function kind(self) -> string throws never
}

class TranscriptDelta {
  text: string,
  implements LiveEvent {
    function kind(self) -> string throws never { "transcript_delta" }
  }
}

class UserSpeechStarted {
  played_ms: int,
  implements LiveEvent {
    function kind(self) -> string throws never { "user_speech_started" }
  }
}

class AssistantAudioStarted {
  item_id: string,
  implements LiveEvent {
    function kind(self) -> string throws never { "assistant_audio_started" }
  }
}

class LiveToolCalls {
  calls: ToolCall[],
  implements LiveEvent {
    function kind(self) -> string throws never { "tool_calls" }
  }
}

class LiveToolResults {
  results: ToolResult[],
  implements LiveEvent {
    function kind(self) -> string throws never { "tool_results" }
  }
}

class LiveClosed {
  implements LiveEvent {
    function kind(self) -> string throws never { "closed" }
  }
}

interface Live requires Resource {
  function events(self) -> LiveEvent[] throws baml.errors.UnknownError
  function submit_tool_results(self, results: ToolResult[]) -> null throws baml.errors.UnknownError
  function cancel_response(self) -> null throws baml.errors.UnknownError
  function truncate_assistant_audio(self, played_ms: int) -> null throws baml.errors.UnknownError
  function close(self) -> null throws never
}

interface RealtimeProvider requires Provider {
  function open_live<T>(self, task: Task<T>, channel: Channel) -> Live
    throws baml.errors.UnknownError
}

// A deterministic Live used by unit tests. Control frames are recorded through
// the same Channel contract as a real provider so ordering can be asserted.
class BufferedLive {
  items: LiveEvent[],
  channel: Channel,
  is_closed: bool,
  control_frames: string[],

  // function cleanup(self) -> void throws never { self.close(); }

  implements Live {
    function events(self) -> LiveEvent[] throws never { self.items }
    function submit_tool_results(self, results: ToolResult[]) -> null throws never {
      let frame = baml.json.stringify({ "type": "tool.results", "count": results.length() });
      self.control_frames.push(frame);
      self.channel.send(frame) catch (e) { _ => null };
      null
    }
    function cancel_response(self) -> null throws never {
      let frame = baml.json.stringify({ "type": "response.cancel" });
      self.control_frames.push(frame);
      self.channel.send(frame) catch (e) { _ => null };
      null
    }
    function truncate_assistant_audio(self, played_ms: int) -> null throws never {
      let frame = baml.json.stringify({
        "type": "conversation.item.truncate",
        "audio_end_ms": played_ms,
      });
      self.control_frames.push(frame);
      self.channel.send(frame) catch (e) { _ => null };
      null
    }
    function close(self) -> null throws never {
      if (!self.is_closed) {
        self.channel.close();
        self.is_closed = true;
      }
      null
    }
  }
  implements Resource {}
}

class FakeRealtimeProvider {
  transcript: string,
  implements Provider {
    function name(self) -> string throws never { "fake-realtime" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }
  implements RealtimeProvider {
    function open_live<T>(self, task: Task<T>, channel: Channel) -> Live throws never {
      channel.send(baml.json.stringify({ "type": "session.created" })) catch (e) { _ => null };
      BufferedLive {
        items: [TranscriptDelta { text: self.transcript }, LiveClosed {}],
        channel: channel,
        is_closed: false,
        control_frames: [],
      }
    }
  }
}

class OpenAiRealtime {
  model: string,
  voice: string,
  api_key: string,
  // null/false is text-only. true requests audio plus its provider transcript.
  output_audio: bool?,
  implements Provider {
    function name(self) -> string throws never { "openai-realtime" }
    function render_shorthand(self) -> string throws never { "openai/" + self.model }
  }
}

function openai_realtime_tool_schemas(tools: Tool[]) -> json[]
    throws never {
  tools.map((tool: Tool) -> json {
    {
      "type": "function",
      "name": tool.name,
      "description": tool.description,
      "parameters": tool.input_schema,
    }
  })
}

function openai_realtime_response_create(
  tools: Tool[],
  output_audio: bool,
  voice: string,
) -> string throws never {
  let schemas = openai_realtime_tool_schemas(tools);
  if (output_audio) {
    if (schemas.length() > 0) {
      baml.json.stringify({
        "type": "response.create",
        "response": {
          "output_modalities": ["audio"],
          "audio": { "output": { "voice": voice } },
          "tools": schemas,
          "tool_choice": "auto",
        },
      })
    } else {
      baml.json.stringify({
        "type": "response.create",
        "response": {
          "output_modalities": ["audio"],
          "audio": { "output": { "voice": voice } },
        },
      })
    }
  } else {
    if (schemas.length() > 0) {
      baml.json.stringify({
        "type": "response.create",
        "response": {
          "output_modalities": ["text"],
          "tools": schemas,
          "tool_choice": "auto",
        },
      })
    } else {
      baml.json.stringify({
        "type": "response.create",
        "response": { "output_modalities": ["text"] },
      })
    }
  }
}

// The WebSocket remains inside this Resource. events(), controls, and tool
// submission therefore all address the same provider session until it closes.
class OpenAiRealtimeLive {
  socket: baml.ws.WsStream?,
  channel: Channel,
  tools: Tool[],
  voice: string,
  output_audio: bool,
  items: LiveEvent[],
  submitted_results: ToolResult[],
  is_closed: bool,
  audio_boundary_emitted: bool,
  last_assistant_item_id: string?,
  cancel_requests: int,
  truncate_positions_ms: int[],

  // function cleanup(self) -> void throws never { self.close(); }

  function send_frame(self, frame: string, label: string) -> null
      throws baml.errors.UnknownError {
    match (self.socket) {
      let ws: baml.ws.WsStream => {
        ws.send(frame) catch (e) {
          _ => throw baml.errors.UnknownError { data: e, message: [label] },
        };
        self.channel.send(frame) catch (e) { _ => null };
        null
      },
      null => throw baml.errors.UnknownError {
        data: null,
        message: ["realtime session is closed"],
      },
    }
  }

  function remember(self, batch: LiveEvent[]) -> LiveEvent[] throws never {
    for (let item in batch) { self.items.push(item); }
    batch
  }

  implements Live {
    // Return one meaningful event batch. Audio sessions yield as soon as the
    // assistant item exists, allowing the caller to cancel/truncate in flight.
    // Tool sessions yield after response.done with correlated LiveToolCalls.
    function events(self) -> LiveEvent[] throws baml.errors.UnknownError {
      if (self.is_closed) { return []; }
      let ws = match (self.socket) {
        let value: baml.ws.WsStream => value,
        null => {
          self.is_closed = true;
          return self.remember([LiveClosed {}]);
        },
      };
      let batch: LiveEvent[] = [];
      let calls: ToolCall[] = [];
      let batch_done = false;
      //# Receive one realtime event batch
      while (!batch_done) {
        //## Read the next provider event
        let next = ws.next() catch (e) {
          _ => throw baml.errors.UnknownError {
            data: e,
            message: ["realtime receive failed"],
          },
        };
        //## Normalize the provider event
        match (next) {
          null => {
            self.close();
            batch.push(LiveClosed {});
            batch_done = true;
          },
          let raw: string => {
            self.channel.send(raw) catch (e) { _ => null };
            let event = baml.json.parse(raw) catch (e) {
              _ => throw baml.errors.UnknownError {
                data: raw,
                message: ["realtime event JSON failed"],
              },
            };
            let kind = baml.json.path_or(event, ".type", "");
            //### Surface text and audio transcripts uniformly
            if (
              kind == "response.output_text.delta"
              || kind == "response.text.delta"
              || kind == "response.output_audio_transcript.delta"
              || kind == "response.audio_transcript.delta"
            ) {
              batch.push(TranscriptDelta {
                text: baml.json.path_or(event, ".delta", ""),
              });
            }
            //### Retain the assistant item needed for precise truncation
            if (kind == "response.output_item.added") {
              let item_type = baml.json.path_or(event, ".item.type", "");
              if (item_type == "message") {
                self.last_assistant_item_id = baml.json.path_or(event, ".item.id", "");
                if (self.output_audio && !self.audio_boundary_emitted) {
                  self.audio_boundary_emitted = true;
                  batch.push(AssistantAudioStarted {
                    item_id: self.last_assistant_item_id ?? "",
                  });
                  batch_done = true;
                }
              }
            }
            //### Preserve realtime tool-call correlation
            if (kind == "response.function_call_arguments.done") {
              let raw_args = baml.json.path_or(event, ".arguments", "{}");
              let args = baml.json.parse(raw_args) catch (e) {
                _ => throw baml.errors.UnknownError {
                  data: raw_args,
                  message: ["realtime tool arguments were not JSON"],
                },
              };
              calls.push(ToolCall {
                id: baml.json.path_or(event, ".call_id", ""),
                name: baml.json.path_or(event, ".name", ""),
                args: args,
              });
            }
            //### End the batch at a response boundary
            if (kind == "response.done" || kind == "response.completed") {
              if (calls.length() > 0) {
                batch.push(LiveToolCalls { calls: calls });
              } else {
                self.close();
                batch.push(LiveClosed {});
              }
              batch_done = true;
            }
            if (kind == "error" || kind == "response.failed") {
              self.close();
              throw baml.errors.UnknownError {
                data: raw,
                message: ["realtime error event"],
              };
            }
          },
        }
      }
      self.remember(batch)
    }

    function submit_tool_results(self, results: ToolResult[]) -> null
        throws baml.errors.UnknownError {
      //# Splice correlated tool outputs into the open conversation
      for (let result in results) {
        self.submitted_results.push(result);
        self.send_frame(baml.json.stringify({
          "type": "conversation.item.create",
          "item": {
            "type": "function_call_output",
            "call_id": result.id,
            "output": baml.json.stringify(result.output),
          },
        }), "realtime tool result send failed");
      }
      //## Request the provider's continuation separately
      self.send_frame(
        openai_realtime_response_create(self.tools, self.output_audio, self.voice),
        "realtime tool continuation failed",
      );
      self.remember([LiveToolResults { results: results }]);
      null
    }

    function cancel_response(self) -> null throws baml.errors.UnknownError {
      //# Cancel the active provider response, not the whole session
      self.send_frame(
        baml.json.stringify({ "type": "response.cancel" }),
        "realtime response.cancel failed",
      );
      self.cancel_requests = self.cancel_requests + 1;
      null
    }

    function truncate_assistant_audio(self, played_ms: int) -> null
        throws baml.errors.UnknownError {
      let item_id = match (self.last_assistant_item_id) {
        let value: string => value,
        null => throw baml.errors.UnknownError {
          data: played_ms,
          message: ["cannot truncate before an assistant audio item starts"],
        },
      };
      //# Rewrite provider memory to exactly what the caller played
      self.send_frame(baml.json.stringify({
        "type": "conversation.item.truncate",
        "item_id": item_id,
        "content_index": 0,
        "audio_end_ms": played_ms,
      }), "realtime conversation.item.truncate failed");
      self.truncate_positions_ms.push(played_ms);
      null
    }

    function close(self) -> null throws never {
      if (!self.is_closed) {
        match (self.socket) {
          let ws: baml.ws.WsStream => ws.close(),
          null => null,
        };
        self.socket = null;
        self.channel.close();
        self.is_closed = true;
      }
      null
    }
  }

  implements Resource {}
}

implements RealtimeProvider for OpenAiRealtime {
  function open_live<T>(self, task: Task<T>, channel: Channel) -> Live
      throws baml.errors.UnknownError {
    //# Open and retain the realtime WebSocket
    let ws = baml.ws.connect(
      "wss://api.openai.com/v1/realtime?model=" + self.model,
      { "Authorization": "Bearer " + self.api_key },
    ) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["realtime connect failed"],
      },
    };
    let initial_item = baml.json.stringify({
      "type": "conversation.item.create",
      "item": {
        "type": "message",
        "role": "user",
        "content": [{
          "type": "input_text",
          "text": baml.llm.prompt_to_text(task.prompt),
        }],
      },
    });
    //## Add the caller-owned conversation item
    ws.send(initial_item) catch (e) {
      _ => {
        ws.close();
        throw baml.errors.UnknownError {
          data: e,
          message: ["realtime initial item failed"],
        };
      },
    };
    channel.send(initial_item) catch (e) { _ => null };
    let output_audio = self.output_audio ?? false;
    let response_create = openai_realtime_response_create(task.tools, output_audio, self.voice);
    //## Start inference without draining or closing the resource
    ws.send(response_create) catch (e) {
      _ => {
        ws.close();
        throw baml.errors.UnknownError {
          data: e,
          message: ["realtime response.create failed"],
        };
      },
    };
    channel.send(response_create) catch (e) { _ => null };
    OpenAiRealtimeLive {
      socket: ws,
      channel: channel,
      tools: task.tools,
      voice: self.voice,
      output_audio: output_audio,
      items: [],
      submitted_results: [],
      is_closed: false,
      audio_boundary_emitted: false,
      last_assistant_item_id: null,
      cancel_requests: 0,
      truncate_positions_ms: [],
    }
  }
}

function collect_live_text(live: Live) -> string throws baml.errors.UnknownError {
  let text = "";
  let done = false;
  //# Drain event batches until the live session closes
  while (!done) {
    let batch = live.events();
    if (batch.length() == 0) { done = true; }
    for (let event in batch) {
      match (event) {
        let delta: TranscriptDelta => { text = text + delta.text; },
        let closed: LiveClosed => { done = true; },
        _ => {},
      }
    }
  }
  text
}
