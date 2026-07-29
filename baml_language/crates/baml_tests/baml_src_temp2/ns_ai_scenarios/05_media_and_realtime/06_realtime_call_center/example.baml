// Operate a voice call-center agent that can listen, speak, use scheduling
// tools, and clean up its realtime audio session.
function realtime_call_center() -> null {
    let audio = root.ai.testing.BufferedRealtimeAudioDevice {
        input_audio_format: root.ai.testing.fake_realtime_audio_format(),
        output_audio_format: root.ai.testing.fake_realtime_audio_format(),
        input_chunks: [],
        next_input_index: 0,
        output_chunks: [],
        played_ms: 0,
        is_closed: false,
    };
    let channel = root.ai.testing.RecordingChannel { frames: [] };
    let task = _realtime_call_center_task(
        root.ai.testing.FakeRealtimeProvider {
            transcript: "Hello, I can help schedule your meeting.",
        },
        "Introduce yourself and ask what meeting the caller wants to schedule.",
    );
    task.run(runner = root.ai.run.VoiceAgent.new(audio, channel));
    log.info(
        {
            "scenario": "realtime_call_center",
            "recorded_frames": channel.frames.length(),
            "audio_device_closed": audio.is_closed,
        },
    );
    null
}
