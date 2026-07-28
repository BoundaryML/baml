function collect_live_text(live_session: LiveSession) -> string throws baml.errors.UnknownError {
    let text = "";
    let done = false;
    //# Drain event batches until the live session closes
    while (!done) {
        let batch = live_session.receive();
        if (batch.length() == 0) {
            done = true;
        }
        for (let event in batch) {
            match (event) {
                let delta: TranscriptDelta => {
                    text = text + delta.text;
                },
                let closed: LiveClosed => {
                    done = true;
                },
                _ => {},
            }
        }
    }
    text
}

class LiveTurn {
    events: LiveEvent[],
    assistant_transcript: string,
    user_transcript: string?,
    assistant_audio: uint8array[],
    tool_calls: ToolCall[],
    tool_results: ToolResult[],
    is_closed: bool,
}

// Collect one response while preserving its transcript, audio, and tool calls.
function collect_live_turn(live_session: LiveSession) -> LiveTurn throws baml.errors.UnknownError {
    let events: LiveEvent[] = [];
    let assistant_transcript = "";
    let user_transcript: string? = null;
    let assistant_audio: uint8array[] = [];
    let tool_calls: ToolCall[] = [];
    let tool_results: ToolResult[] = [];
    let is_closed = false;
    let done = false;
    //# Collect one logical turn across as many event batches as required
    while (!done) {
        //## Receive the next live event batch
        let batch = live_session.receive();
        if (batch.length() == 0) {
            done = true;
        }
        //## Fold transcript, audio, tools, and terminal events into the turn
        for (let event in batch) {
            events.push(event);
            match (event) {
                let delta: TranscriptDelta => {
                    assistant_transcript = assistant_transcript + delta.text;
                },
                let transcript: UserTranscript => {
                    user_transcript = transcript.text;
                },
                let audio: AssistantAudioDelta => {
                    assistant_audio.push(audio.audio);
                },
                let calls: LiveToolCalls => {
                    //### Return control when raw tool calls require application handling
                    for (let call in calls.calls) {
                        tool_calls.push(call);
                    }
                    done = true;
                },
                let results: LiveToolResults => {
                    //### Keep collecting after automatically executed tool results
                    for (let result in results.results) {
                        tool_results.push(result);
                    }
                    // AutomaticToolLiveSession has already submitted these results and
                    // requested the model's continuation, so keep collecting this turn.
                    done = false;
                },
                let completed: LiveTurnCompleted => {
                    //### Stop at the provider's completed-turn boundary
                    done = true;
                },
                let closed: LiveClosed => {
                    //### Record provider closure as a terminal turn
                    is_closed = true;
                    done = true;
                },
                _ => {},
            }
        }
    }
    LiveTurn {
        events: events,
        assistant_transcript: assistant_transcript,
        user_transcript: user_transcript,
        assistant_audio: assistant_audio,
        tool_calls: tool_calls,
        tool_results: tool_results,
        is_closed: is_closed,
    }
}
