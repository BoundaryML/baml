implements root.ai.RealtimeProvider for Realtime {
  function open_live<T>(
    self,
    task: root.ai.Task<T>,
    channel: root.ai.Channel,
  ) -> root.ai.LiveSession
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
    //## Configure the persistent voice session before adding conversation items
    let session_update = _openai_realtime_session_update(
      _openai_realtime_task_instructions(task.prompt),
      self.voice,
      self.input_transcription_model,
      self.server_vad,
    );
    ws.send(session_update) catch (e) {
      _ => {
        ws.close();
        throw baml.errors.UnknownError {
          data: e,
          message: ["realtime session.update failed"],
        };
      },
    };
    channel.send(session_update) catch (e) { _ => null };
    let initial_text = _openai_realtime_task_input(task.prompt);
    let initial_item = baml.json.stringify({
      "type": "conversation.item.create",
      "item": {
        "type": "message",
        "role": "user",
        "content": [{
          "type": "input_text",
          "text": initial_text,
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
    let response_create = _openai_realtime_response_create(task.tools, output_audio, self.voice);
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
    RealtimeSession {
      socket: ws,
      channel: channel,
      tools: task.tools,
      voice: self.voice,
      output_audio: output_audio,
      keep_open: self.keep_open ?? false,
      input_transcription_enabled: self.input_transcription_model != null,
      automatic_turn_detection: self.server_vad != null,
      items: [],
      submitted_results: [],
      is_closed: false,
      audio_boundary_emitted: false,
      awaiting_input_transcript: false,
      last_assistant_item_id: null,
      assistant_audio_bytes_by_item_id: {},
      pending_tool_calls: [],
      response_finished: false,
      cancel_requests: 0,
      truncate_positions_ms: [],
    }
  }
}

// OpenAI rejects conversation.item.truncate when audio_end_ms is beyond the
// audio retained for that exact item. PCM16 mono at 24 kHz contains 48 bytes/ms.
