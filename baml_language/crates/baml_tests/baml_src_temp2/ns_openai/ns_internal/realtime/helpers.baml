function _realtime_audio_end_ms(locally_played_ms: int, item_audio_bytes: int) -> int throws never {
    let item_duration_ms = item_audio_bytes / 48;
    if (locally_played_ms < 0) {
        0
    } else if (locally_played_ms > item_duration_ms) {
        item_duration_ms
    } else {
        locally_played_ms
    }
}
