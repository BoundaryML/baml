// Operate a voice call-center agent that can listen, speak, use scheduling
// tools, and clean up its realtime audio session.
function realtime_call_center() -> null {
    let audio = ai.testing.BufferedRealtimeAudioDevice {
        input_audio_format: ai.testing.fake_realtime_audio_format(),
        output_audio_format: ai.testing.fake_realtime_audio_format(),
        input_chunks: [],
        next_input_index: 0,
        output_chunks: [],
        played_ms: 0,
        is_closed: false,
    };
    let channel = ai.testing.RecordingChannel { frames: [] };
    let task = RealtimeCallCenter@task("Introduce yourself and ask what meeting the caller wants to schedule.").with_provider(ai.testing.FakeRealtimeProvider {
            transcript: "Hello, I can help schedule your meeting.",
        });
    task.run(runner = ai.run.VoiceAgent.new(audio, channel));
    log.info(
        {
            "scenario": "realtime_call_center",
            "recorded_frames": channel.frames.length(),
            "audio_device_closed": audio.is_closed,
        },
    );
    null
}
