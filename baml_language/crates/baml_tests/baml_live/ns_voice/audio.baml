// Audio devices: where the user's speech comes from and where the assistant's
// speech goes. `MacAudioDevice` is a real microphone and speaker.
// `ScriptedAudioDevice` plays prepared utterances as the user and records the
// assistant, so a conversation can run without audio hardware.
//
// All audio is raw PCM16, 24 kHz, mono, little-endian: 48 bytes per millisecond.

interface AudioDevice {
    /// Block until the next microphone frame. Null means the input ended.
    function next_input(self) -> uint8array? throws baml.errors.Io;

    /// Queue assistant audio for playback and return without waiting for it.
    function play(self, pcm: uint8array) -> void throws baml.errors.Io;

    /// Discard the queued playback. Returns how many milliseconds of the
    /// current utterance had been heard.
    function stop_playback(self) -> int throws baml.errors.Io;

    /// Block until the queued playback has been heard.
    function drain(self) -> void throws baml.errors.Io;

    function close(self) -> void throws never;
}

/// Tracks how much queued audio has been heard. Audio is queued faster than
/// it plays, so the heard position is derived from the wall clock.
class PlaybackClock {
    _origin: baml.time.Instant,
    // When the queue runs dry, in milliseconds since `_origin`.
    _end_ms: int,
    _utterance_start_ms: int,
    _utterance_ms: int,

    function new() -> PlaybackClock {
        PlaybackClock {
            _origin: baml.time.Instant.now(),
            _end_ms: 0,
            _utterance_start_ms: 0,
            _utterance_ms: 0,
        }
    }

    /// Milliseconds since this clock was created.
    function now_ms(self) -> int throws never {
        self._origin.elapsed().to_milliseconds().to_int() catch_all (e) {
            _ => 0,
        }
    }

    /// Account for queued audio. `new_utterance` marks the first audio of a
    /// new assistant item, which starts playing when the queue ahead of it
    /// runs dry.
    function queue(self, byte_count: int, new_utterance: bool = false) -> void throws never {
        let now = self.now_ms();
        if (now >= self._end_ms) {
            self._end_ms = now;
            self._utterance_start_ms = now;
            self._utterance_ms = 0;
        } else if (new_utterance) {
            self._utterance_start_ms = self._end_ms;
            self._utterance_ms = 0;
        }
        let ms = byte_count / 48;
        self._end_ms = self._end_ms + ms;
        self._utterance_ms = self._utterance_ms + ms;
    }

    /// Empty the queue. Returns the milliseconds of the utterance already heard.
    function stop(self) -> int throws never {
        let now = self.now_ms();
        let heard = now - self._utterance_start_ms;
        let clamped = if (heard < 0) {
            0
        } else if (heard > self._utterance_ms) {
            self._utterance_ms
        } else {
            heard
        };
        self._end_ms = now;
        self._utterance_ms = 0;
        clamped
    }

    function remaining_ms(self) -> int throws never {
        let left = self._end_ms - self.now_ms();
        if (left > 0) {
            left
        } else {
            0
        }
    }
}

/// True when `pcm` holds no sample louder than about -42 dBFS. GPT-Live
/// streams digital silence while the model is quiet. Samples every 16th frame.
function is_silent(pcm: uint8array) -> bool throws never {
    let i = 1;
    while (i < pcm.length()) {
        // The high byte of a quiet sample is 0x00 (positive) or 0xff (negative).
        let high = pcm.at(i) ?? 0;
        if (high != 0 && high != 255) {
            return false;
        }
        i = i + 32;
    }
    true
}

function _le(value: int, width: int) -> int[] {
    let out: int[] = [];
    let rest = value;
    let i = 0;
    while (i < width) {
        out.push(rest % 256);
        rest = rest / 256;
        i = i + 1;
    }
    out
}

/// A WAV file holding `pcm`, for listening to a recorded conversation.
function wav(pcm: uint8array) -> uint8array throws baml.errors.InvalidArgument {
    let header: int[] = [];
    for (let b in "RIFF".to_utf8().to_array()) {
        header.push(b);
    }
    for (let b in _le(36 + pcm.length(), 4)) {
        header.push(b);
    }
    for (let b in "WAVEfmt ".to_utf8().to_array()) {
        header.push(b);
    }
    for (let b in _le(16, 4)) {
        header.push(b);
    } // fmt chunk size
    for (let b in _le(1, 2)) {
        header.push(b);
    } // PCM
    for (let b in _le(1, 2)) {
        header.push(b);
    } // mono
    for (let b in _le(24000, 4)) {
        header.push(b);
    } // sample rate
    for (let b in _le(48000, 4)) {
        header.push(b);
    } // byte rate
    for (let b in _le(2, 2)) {
        header.push(b);
    } // block align
    for (let b in _le(16, 2)) {
        header.push(b);
    } // bits per sample
    for (let b in "data".to_utf8().to_array()) {
        header.push(b);
    }
    for (let b in _le(pcm.length(), 4)) {
        header.push(b);
    }
    baml.Uint8Array.from_array(header).concat(pcm)
}

// ---------------------------------------------------------------------------
// macOS microphone and speaker

/// (internal) The Swift helper's source file. `BAML_LIVE_AUDIO_HELPER` overrides the
/// search, which covers running from the package, the workspace, and the repo.
function _helper_source() -> string throws baml.errors.Io {
    let configured = baml.env.get("BAML_LIVE_AUDIO_HELPER") catch_all (e) {
        _ => null,
    };
    if let path: string = configured {
        return path;
    }
    let name = "support/duplex_audio_macos.swift";
    let candidates = [
        name,
        "baml_live/" + name,
        "crates/baml_tests/baml_live/" + name,
        "baml_language/crates/baml_tests/baml_live/" + name,
    ];
    for (let c in candidates) {
        if (baml.fs.exists(c)) {
            return c;
        }
    }
    throw baml.errors.Io {
        message: `could not find ${name}; run from the baml_live package or set BAML_LIVE_AUDIO_HELPER to its path`,
    }
}

/// (internal) Compile the helper once and reuse the binary. The source size is part of
/// the name, so an edited helper is rebuilt.
function _helper_binary() -> string throws baml.errors.Io {
    let source = _helper_source();
    let tmp = (
        baml.env.get("TMPDIR") catch_all (e) {
            _ => null,
        }
    )
        ?? "/tmp";
    let dir = if (tmp.ends_with("/")) {
        tmp + "baml_live"
    } else {
        tmp + "/baml_live"
    };
    let binary = `${dir}/duplex_audio_macos_${baml.fs.size(source)}`;
    if (baml.fs.exists(binary)) {
        return binary;
    }
    baml.io.eprintln(
        "Compiling the macOS audio helper. This takes about 30 seconds the first time.",
    );
    let built = baml.sys.shell(`mkdir -p '${dir}' && /usr/bin/xcrun swiftc -O '${source}' -o '${binary}'`, null) catch_all (e) {
        _ => {
            throw baml.errors.Io { message: `could not run swiftc: ${e.to_string()}` }
        },
    };
    if (!built.ok()) {
        throw baml.errors.Io {
            message: `swiftc failed to build ${source}: ${built.stderr.to_string()}`,
        }
    }
    binary
}

/// The Mac's default microphone and speakers, full duplex, with acoustic echo
/// cancellation. `echo_cancellation = false` skips macOS voice processing;
/// use it with headphones.
class MacAudioDevice {
    process: baml.sys.Process,
    _frames: baml.sys.ReadPipeLines,
    _clock: PlaybackClock,
    _closed: bool,

    function new(echo_cancellation: bool = true) -> MacAudioDevice throws baml.errors.Io {
        let args: string[] = if (echo_cancellation) {
            []
        } else {
            ["--raw"]
        };
        let process = baml.sys.start_process(_helper_binary(), args, null);
        MacAudioDevice {
            process: process,
            _frames: process.stdout.lines(),
            _clock: PlaybackClock.new(),
            _closed: false,
        }
    }

    function _command(self, line: string) -> void throws baml.errors.Io {
        let _ = self.process.stdin.write(line + "\n");
        self.process.stdin.flush();
    }

    implements AudioDevice {
        function next_input(self) -> uint8array? {
            if (self._closed) {
                return null;
            }
            match (self._frames.next()) {
                let line: string => {
                    baml.Uint8Array.from_base64(line) catch_all (e) {
                        _ => b"",
                    }
                },
                let done: baml.iter.Done => null,
            }
        }

        function play(self, pcm: uint8array) -> void {
            if (self._closed || pcm.length() == 0) {
                return;
            }
            self._command("audio " + pcm.to_base64());
            self._clock.queue(pcm.length());
        }

        function stop_playback(self) -> int {
            let heard = self._clock.stop();
            if (!self._closed) {
                self._command("stop");
            }
            heard
        }

        function drain(self) -> void {
            let left = self._clock.remaining_ms();
            if (left > 0) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(left));
            }
        }

        function close(self) -> void {
            if (self._closed) {
                return;
            }
            self._closed = true;
            self.process.close();
        }
    }
}

// ---------------------------------------------------------------------------
// Scripted caller

/// One line of a scripted caller.
class ScriptedLine {
    pcm: uint8array,
    /// The line waits until `ScriptedAudioDevice.cue` names this cue. A
    /// person waits for the booking to go through before saying thanks; a
    /// script has to be told. Null takes the turn as soon as it is free.
    after_cue: string?,
}

/// A caller with a script. It delivers `lines` in order as microphone audio
/// at real-time pace, with silence in between, and records everything the
/// assistant plays. A line starts once the assistant has spoken and then
/// stayed quiet for `gap_ms`, which is how a person takes turns, and once its
/// cue has arrived. A line whose turn never comes starts after `patience_ms`
/// of quiet, so a script cannot stall a conversation forever.
class ScriptedAudioDevice {
    lines: ScriptedLine[],
    gap_ms: int,
    patience_ms: int,
    /// Every chunk the assistant played, in order.
    output: uint8array[],
    _cues: string[],
    _clock: PlaybackClock,
    _frames_sent: int,
    _next: int,
    _speaking: uint8array?,
    _offset: int,
    _assistant_answered: bool,
    _closed: bool,

    function new(
        lines: ScriptedLine[],
        gap_ms: int = 1200,
        patience_ms: int = 20000,
    ) -> ScriptedAudioDevice {
        ScriptedAudioDevice {
            lines: lines,
            gap_ms: gap_ms,
            patience_ms: patience_ms,
            output: [],
            _cues: [],
            _clock: PlaybackClock.new(),
            _frames_sent: 0,
            _next: 0,
            _speaking: null,
            _offset: 0,
            _assistant_answered: false,
            _closed: false,
        }
    }

    /// Release the lines waiting on `name`.
    function cue(self, name: string) -> void throws never {
        self._cues.push(name);
    }

    /// Everything the assistant played, as one PCM buffer.
    function recorded(self) -> uint8array {
        let all = b"";
        for (let chunk in self.output) {
            all = all.concat(chunk);
        }
        all
    }

    /// True once every line has been delivered in full.
    function finished_speaking(self) -> bool {
        self._next >= self.lines.length() && self._speaking == null
    }

    function _ready_for_next(self, now: int) -> bool {
        let line: ScriptedLine = self.lines.at(self._next) else {
            return false;
        };
        let quiet_ms = now - self._clock._end_ms;
        if (quiet_ms >= self.patience_ms) {
            return true;
        }
        let cued = match (line.after_cue) {
            let name: string => self._cues.includes(name),
            null => true,
        };
        cued && self._assistant_answered && quiet_ms >= self.gap_ms
    }

    implements AudioDevice {
        function next_input(self) -> uint8array? {
            if (self._closed) {
                return null;
            }
            // One 100 ms frame per 100 ms of wall clock, like a microphone.
            let due = self._frames_sent * 100;
            let now = self._clock.now_ms();
            if (due > now) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(due - now));
            }
            self._frames_sent = self._frames_sent + 1;
            if (self._speaking == null && self._ready_for_next(self._clock.now_ms())) {
                self._speaking = self.lines.at(self._next)?.pcm;
                self._next = self._next + 1;
                self._offset = 0;
            }
            if let speech: uint8array = self._speaking {
                let end = if (self._offset + 4800 < speech.length()) {
                    self._offset + 4800
                } else {
                    speech.length()
                };
                let frame = speech.slice(self._offset, end);
                self._offset = end;
                if (end >= speech.length()) {
                    self._speaking = null;
                    self._assistant_answered = false;
                }
                return frame;
            }
            baml.Uint8Array.zeroes(4800) catch_all (e) {
                _ => b"",
            }
        }

        function play(self, pcm: uint8array) -> void {
            self.output.push(pcm);
            if (!is_silent(pcm)) {
                self._clock.queue(pcm.length());
                self._assistant_answered = true;
            }
        }

        function stop_playback(self) -> int {
            self._clock.stop()
        }

        function drain(self) -> void {
            let left = self._clock.remaining_ms();
            if (left > 0) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(left));
            }
        }

        function close(self) -> void {
            self._closed = true;
        }
    }
}
