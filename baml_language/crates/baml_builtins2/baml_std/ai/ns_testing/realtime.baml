// Deterministic realtime resources used by integration tests and examples.
function fake_realtime_audio_format() -> ai.realtime.RealtimeAudioFormat throws never {
    ai.realtime.RealtimeAudioFormat { encoding: "test-bytes", sample_rate_hz: 1, channels: 1 }
}

class RecordingChannel {
    frames: string[],

    implements ai.realtime.Channel {
        function send(self, frame: string) -> null throws never {
            self.frames.push(frame);
            null
        }

        function close(self) -> null throws never {
            null
        }
    }
}

class BufferedLiveSession {
    items: ai.realtime.LiveEvent[],
    channel: ai.realtime.Channel,
    is_closed: bool,
    control_frames: string[],

    // function cleanup(self) -> void throws never { self.close(); }

    implements ai.realtime.LiveSession {
        function receive(self) -> ai.realtime.LiveEvent[] throws never {
            self.items
        }

        function send_text(self, text: string) -> null throws never {
            let frame = baml.json.stringify({ "type": "conversation.item.create", "text": text });
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
            null
        }

        function input_audio_format(self) -> ai.realtime.RealtimeAudioFormat throws never {
            fake_realtime_audio_format()
        }

        function output_audio_format(self) -> ai.realtime.RealtimeAudioFormat throws never {
            fake_realtime_audio_format()
        }

        function send_audio(self, audio: uint8array) -> null throws never {
            let frame = baml.json.stringify({ "type": "input_audio_buffer.append", "bytes": audio.length() });
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
            null
        }

        function commit_audio(self) -> null throws never {
            let frame = baml.json.stringify({ "type": "input_audio_buffer.commit" });
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
            null
        }

        function send_audio_turn(self, audio: uint8array) -> null throws never {
            let frame = baml.json.stringify({ "type": "input_audio_buffer.commit", "bytes": audio.length() });
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
            null
        }

        function submit_tool_results(
            self,
            results: ai.tools.ToolResult[],
        ) -> null throws never {
            let frame = baml.json.stringify({ "type": "tool.results", "count": results.length() });
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
            null
        }

        function request_response(self) -> null throws never {
            let frame = baml.json.stringify({ "type": "response.create" });
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
            null
        }

        function cancel_response(self) -> null throws never {
            let frame = baml.json.stringify({ "type": "response.cancel" });
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
            null
        }

        function truncate_assistant_audio(self, played_ms: int) -> null throws never {
            let frame = baml.json.stringify({ "type": "conversation.item.truncate", "audio_end_ms": played_ms });
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
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

    implements ai.Resource {}
}

class FakeRealtimeProvider {
    transcript: string,

    implements ai.Provider {
        function name(self) -> string throws never {
            "fake-realtime"
        }
        function render_shorthand(self) -> string throws never {
            _fake_render_shorthand()
        }
    }

    implements ai.realtime.RealtimeProvider {
        function open_live<T>(
            self,
            task: ai.Task<T>,
            channel: ai.realtime.Channel,
        ) -> ai.realtime.LiveSession throws never {
            channel.send(baml.json.stringify({ "type": "session.created" })) catch (e) {
                _ => null
            };
            BufferedLiveSession {
                items: [
                    ai.realtime.TranscriptDelta { text: self.transcript },
                    ai.realtime.LiveClosed {  },
                ],
                channel: channel,
                is_closed: false,
                control_frames: [],
            }
        }
    }
}

class BufferedRealtimeAudioDevice {
    input_audio_format: ai.realtime.RealtimeAudioFormat,
    output_audio_format: ai.realtime.RealtimeAudioFormat,
    input_chunks: uint8array[],
    next_input_index: int,
    output_chunks: uint8array[],
    played_ms: int,
    is_closed: bool,

    implements ai.realtime.RealtimeAudioDevice {
        function input_format(self) -> ai.realtime.RealtimeAudioFormat throws never {
            self.input_audio_format
        }

        function output_format(self) -> ai.realtime.RealtimeAudioFormat throws never {
            self.output_audio_format
        }

        function next_input(self) -> uint8array? throws never {
            if (self.next_input_index >= self.input_chunks.length()) {
                return null;
            }
            let chunk = self.input_chunks[self.next_input_index];
            self.next_input_index = self.next_input_index + 1;
            chunk
        }

        function play_output(self, audio: uint8array) -> null throws never {
            self.output_chunks.push(audio);
            null
        }

        function finish_output(self) -> null throws never {
            null
        }

        function stop_output(self) -> int throws never {
            let heard = self.played_ms;
            self.output_chunks = [];
            heard
        }

        function close(self) -> null throws never {
            self.is_closed = true;
            null
        }
    }

    implements ai.Resource {}
}
