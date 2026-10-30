// Full-duplex audio device for the baml_live voice demo on macOS.
//
// BAML starts this program as a child process (`voice.MacAudioDevice`) and
// speaks a line protocol with it. All audio is PCM16, 24 kHz, mono,
// little-endian, Base64 encoded, one frame per line.
//
//   stdout   BASE64            one microphone frame (about 20 to 40 ms)
//   stdin    audio BASE64      queue assistant audio for playback
//            stop              discard queued playback (the user interrupted)
//            EOF               shut down
//
// Capture and playback share one AVAudioEngine in voice-processing mode, so
// macOS uses the audio sent to the speakers as the reference signal for
// acoustic echo cancellation. The microphone stays live while the assistant
// speaks and the assistant does not hear itself. Pass `--raw` to skip voice
// processing, for example with headphones.
//
// Build: swiftc -O duplex_audio_macos.swift -o duplex_audio_macos

import AVFoundation
import Foundation

private let sampleRate = 24_000.0
private let channels: AVAudioChannelCount = 1

private func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data(("error: \(message)\n").utf8))
    exit(1)
}

private func microphoneIsAuthorized() -> Bool {
    switch AVCaptureDevice.authorizationStatus(for: .audio) {
    case .authorized:
        return true
    case .notDetermined:
        let semaphore = DispatchSemaphore(value: 0)
        var allowed = false
        AVCaptureDevice.requestAccess(for: .audio) { granted in
            allowed = granted
            semaphore.signal()
        }
        semaphore.wait()
        return allowed
    default:
        return false
    }
}

private final class DuplexAudioDevice {
    private let engine = AVAudioEngine()
    private let player = AVAudioPlayerNode()
    private let wireFormat: AVAudioFormat
    private let converter: AVAudioConverter
    private let stdoutLock = NSLock()

    init(voiceProcessing: Bool) {
        guard microphoneIsAuthorized() else {
            fail("microphone access is disabled; allow your terminal app in System Settings > Privacy & Security > Microphone")
        }

        let input = engine.inputNode
        _ = engine.outputNode
        if voiceProcessing {
            do {
                // The engine must be stopped while its I/O nodes switch into
                // voice-processing mode.
                try input.setVoiceProcessingEnabled(true)
                try engine.outputNode.setVoiceProcessingEnabled(true)
                input.isVoiceProcessingInputMuted = false
                input.isVoiceProcessingBypassed = false
            } catch {
                fail("could not enable macOS voice processing: \(error); retry with --raw")
            }
        }

        let inputFormat = input.outputFormat(forBus: 0)
        guard inputFormat.channelCount > 0, inputFormat.sampleRate > 0 else {
            fail("no microphone input format is available")
        }
        guard let format = AVAudioFormat(
            commonFormat: .pcmFormatInt16,
            sampleRate: sampleRate,
            channels: channels,
            interleaved: false
        ) else {
            fail("could not create the 24 kHz PCM format")
        }
        wireFormat = format
        guard let inputConverter = AVAudioConverter(from: inputFormat, to: format) else {
            fail("could not create the microphone sample-rate converter")
        }
        if inputFormat.channelCount > 1 {
            // Voice-processing I/O exposes the processed microphone on channel
            // 0 and echo-reference channels after it.
            inputConverter.channelMap = [0]
        }
        converter = inputConverter

        engine.attach(player)
        engine.connect(player, to: engine.mainMixerNode, format: format)
        if voiceProcessing {
            engine.connect(engine.mainMixerNode, to: engine.outputNode, format: inputFormat)
        }
        input.installTap(onBus: 0, bufferSize: 960, format: inputFormat) {
            [converter, wireFormat, stdoutLock] buffer, _ in
            let ratio = wireFormat.sampleRate / inputFormat.sampleRate
            let capacity = AVAudioFrameCount(ceil(Double(buffer.frameLength) * ratio)) + 16
            guard let converted = AVAudioPCMBuffer(pcmFormat: wireFormat, frameCapacity: capacity) else {
                return
            }
            var supplied = false
            var conversionError: NSError?
            let status = converter.convert(to: converted, error: &conversionError) { _, statusPointer in
                if supplied {
                    statusPointer.pointee = .noDataNow
                    return nil
                }
                supplied = true
                statusPointer.pointee = .haveData
                return buffer
            }
            guard status != .error, conversionError == nil else {
                return
            }
            let audioBuffer = converted.audioBufferList.pointee.mBuffers
            guard let bytes = audioBuffer.mData, audioBuffer.mDataByteSize > 0 else {
                return
            }
            let pcm = Data(bytes: bytes, count: Int(audioBuffer.mDataByteSize))
            stdoutLock.lock()
            FileHandle.standardOutput.write(Data((pcm.base64EncodedString() + "\n").utf8))
            stdoutLock.unlock()
        }
    }

    func start() {
        do {
            engine.prepare()
            try engine.start()
        } catch {
            engine.inputNode.removeTap(onBus: 0)
            fail("could not start duplex audio: \(error)")
        }
    }

    func play(base64: String) {
        guard let data = Data(base64Encoded: base64), !data.isEmpty,
              data.count % MemoryLayout<Int16>.size == 0 else {
            return
        }
        let frames = AVAudioFrameCount(data.count / MemoryLayout<Int16>.size)
        guard let buffer = AVAudioPCMBuffer(pcmFormat: wireFormat, frameCapacity: frames),
              let destination = buffer.int16ChannelData?[0] else {
            return
        }
        _ = data.copyBytes(to: UnsafeMutableBufferPointer(start: destination, count: Int(frames)))
        buffer.frameLength = frames
        player.scheduleBuffer(buffer)
        if !player.isPlaying {
            player.play()
        }
    }

    func stopPlayback() {
        // Stopping the node discards every scheduled buffer.
        player.stop()
    }

    func close() {
        player.stop()
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
    }
}

// A closed stdout means the BAML process went away.
signal(SIGPIPE, SIG_IGN)

let arguments = CommandLine.arguments.dropFirst()
if arguments.contains("--help") || arguments.contains("-h") {
    print("usage: duplex_audio_macos [--raw]")
    exit(0)
}

private let device = DuplexAudioDevice(voiceProcessing: !arguments.contains("--raw"))
device.start()
FileHandle.standardError.write(Data("duplex audio ready\n".utf8))
while let command = readLine(strippingNewline: true) {
    if command == "stop" {
        device.stopPlayback()
    } else if command.hasPrefix("audio ") {
        device.play(base64: String(command.dropFirst("audio ".count)))
    }
}
device.close()
