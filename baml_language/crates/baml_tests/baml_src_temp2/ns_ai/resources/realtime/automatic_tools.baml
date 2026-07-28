class AutomaticToolLiveSession {
    inner: LiveSession,
    tools: Tool[],

    implements LiveSession {
        function receive(self) -> LiveEvent[] throws baml.errors.UnknownError {
            //# Receive one provider event batch
            let events = self.inner.receive();
            let calls: ToolCall[] = [];
            //# Collect every tool call proposed in this batch
            for (let event in events) {
                match (event) {
                    let requested: LiveToolCalls => {
                        for (let call in requested.calls) {
                            calls.push(call);
                        }
                    },
                    _ => {},
                }
            }
            if (calls.length() == 0) {
                //## Return immediately when no application tools are requested
                return events;
            }
            //# Execute the task's concrete tools, preserving provider call IDs
            let results = calls.map((call: ToolCall) -> ToolResult {
                match (
                    self.tools.find((tool: Tool) -> bool {
                        tool.name == call.name
                    })
                ) {
                    let tool: Tool => invoke_tool(tool, call),
                    null => ToolResult.error(call, "tool is not registered on this task"),
                }
            });
            //## Add correlated results to the provider transcript and continue
            self.inner.submit_tool_results(results);
            events.push(LiveToolResults { results: results });
            events
        }

        function send_text(self, text: string) -> null throws baml.errors.UnknownError {
            self.inner.send_text(text)
        }

        function input_audio_format(self) -> RealtimeAudioFormat throws never {
            self.inner.input_audio_format()
        }

        function output_audio_format(self) -> RealtimeAudioFormat throws never {
            self.inner.output_audio_format()
        }

        function send_audio(self, audio: uint8array) -> null throws baml.errors.UnknownError {
            self.inner.send_audio(audio)
        }

        function commit_audio(self) -> null throws baml.errors.UnknownError {
            self.inner.commit_audio()
        }

        function send_audio_turn(self, audio: uint8array) -> null throws baml.errors.UnknownError {
            self.inner.send_audio_turn(audio)
        }

        function submit_tool_results(
            self,
            results: ToolResult[],
        ) -> null throws baml.errors.UnknownError {
            self.inner.submit_tool_results(results)
        }

        function request_response(self) -> null throws baml.errors.UnknownError {
            self.inner.request_response()
        }

        function cancel_response(self) -> null throws baml.errors.UnknownError {
            self.inner.cancel_response()
        }

        function truncate_assistant_audio(
            self,
            played_ms: int,
        ) -> null throws baml.errors.UnknownError {
            self.inner.truncate_assistant_audio(played_ms)
        }

        function close(self) -> null throws never {
            self.inner.close()
        }
    }
    implements Resource {}
}

// Application-owned audio I/O. next_input() yields short frames in the
// device's declared input format; play_output() queues bytes in its declared
// output format without blocking reception.
// stop_output() drops queued audio and reports how many milliseconds the caller
// actually heard from the current assistant item.
