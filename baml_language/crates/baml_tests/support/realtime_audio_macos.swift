#!/usr/bin/env swift

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

private func record(to path: String, maximumSeconds: Double, silenceSeconds: Double) {
    guard microphoneIsAuthorized() else {
        fail("microphone access is disabled; allow Terminal (or your shell app) in System Settings > Privacy & Security > Microphone")
    }

    FileManager.default.createFile(atPath: path, contents: nil)
    guard let output = FileHandle(forWritingAtPath: path) else {
        fail("could not open \(path) for writing")
    }
    defer { try? output.close() }

    let engine = AVAudioEngine()
    let input = engine.inputNode
    let inputFormat = input.outputFormat(forBus: 0)
    guard inputFormat.channelCount > 0, inputFormat.sampleRate > 0 else {
        fail("no microphone input format is available")
    }
    guard let wireFormat = AVAudioFormat(
        commonFormat: .pcmFormatInt16,
        sampleRate: sampleRate,
        channels: channels,
        interleaved: true
    ) else {
        fail("could not create the 24 kHz PCM output format")
    }
    guard let converter = AVAudioConverter(from: inputFormat, to: wireFormat) else {
        fail("could not create the microphone sample-rate converter")
    }

    let stateLock = NSLock()
    var heardSpeech = false
    var lastSpeech = Date()
    var shouldStop = false
    let startedAt = Date()
    let speechThreshold: Float = 0.012

    input.installTap(onBus: 0, bufferSize: 4_096, format: inputFormat) { buffer, _ in
        if let samples = buffer.floatChannelData?[0] {
            var sum: Float = 0
            for index in 0..<Int(buffer.frameLength) {
                let value = samples[index]
                sum += value * value
            }
            let rms = buffer.frameLength == 0 ? 0 : sqrt(sum / Float(buffer.frameLength))
            stateLock.lock()
            if rms >= speechThreshold {
                heardSpeech = true
                lastSpeech = Date()
            } else if heardSpeech && Date().timeIntervalSince(lastSpeech) >= silenceSeconds {
                shouldStop = true
            }
            stateLock.unlock()
        }

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
            stateLock.lock()
            shouldStop = true
            stateLock.unlock()
            return
        }
        let audioBuffer = converted.audioBufferList.pointee.mBuffers
        guard let bytes = audioBuffer.mData, audioBuffer.mDataByteSize > 0 else {
            return
        }
        output.write(Data(bytes: bytes, count: Int(audioBuffer.mDataByteSize)))
    }

    do {
        engine.prepare()
        try engine.start()
    } catch {
        input.removeTap(onBus: 0)
        fail("could not start microphone capture: \(error)")
    }

    while Date().timeIntervalSince(startedAt) < maximumSeconds {
        stateLock.lock()
        let stop = shouldStop
        stateLock.unlock()
        if stop { break }
        RunLoop.current.run(until: Date().addingTimeInterval(0.05))
    }

    engine.stop()
    input.removeTap(onBus: 0)
    try? output.synchronize()
}

// Emit short PCM16/24 kHz/mono microphone frames as Base64 JSONL-friendly
// lines. OpenAI Realtime receives these continuously and owns turn detection.
private func streamMicrophone() {
    guard microphoneIsAuthorized() else {
        fail("microphone access is disabled; allow Terminal (or your shell app) in System Settings > Privacy & Security > Microphone")
    }

    let engine = AVAudioEngine()
    let input = engine.inputNode
    let inputFormat = input.outputFormat(forBus: 0)
    guard inputFormat.channelCount > 0, inputFormat.sampleRate > 0 else {
        fail("no microphone input format is available")
    }
    guard let wireFormat = AVAudioFormat(
        commonFormat: .pcmFormatInt16,
        sampleRate: sampleRate,
        channels: channels,
        interleaved: true
    ) else {
        fail("could not create the 24 kHz PCM output format")
    }
    guard let converter = AVAudioConverter(from: inputFormat, to: wireFormat) else {
        fail("could not create the microphone sample-rate converter")
    }

    input.installTap(onBus: 0, bufferSize: 960, format: inputFormat) { buffer, _ in
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
        let line = pcm.base64EncodedString() + "\n"
        FileHandle.standardOutput.write(Data(line.utf8))
    }

    do {
        engine.prepare()
        try engine.start()
    } catch {
        input.removeTap(onBus: 0)
        fail("could not start microphone streaming: \(error)")
    }

    while true {
        RunLoop.current.run(until: Date().addingTimeInterval(1))
    }
}

// Capture and playback share one voice-processing audio engine. macOS can
// therefore use the audio sent to the speakers as the reference signal for
// acoustic echo cancellation while keeping the microphone fully live.
private final class DuplexAudioDevice {
    private let engine = AVAudioEngine()
    private let player = AVAudioPlayerNode()
    private let wireFormat: AVAudioFormat
    private let converter: AVAudioConverter

    init() {
        guard microphoneIsAuthorized() else {
            fail("microphone access is disabled; allow Terminal (or your shell app) in System Settings > Privacy & Security > Microphone")
        }

        let input = engine.inputNode
        _ = engine.outputNode
        do {
            // Apple requires the engine to be stopped while switching its I/O
            // nodes into voice-processing mode.
            try input.setVoiceProcessingEnabled(true)
            try engine.outputNode.setVoiceProcessingEnabled(true)
            input.isVoiceProcessingInputMuted = false
            input.isVoiceProcessingBypassed = false
        } catch {
            fail("could not enable macOS voice processing: \(error)")
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
            fail("could not create the 24 kHz PCM duplex format")
        }
        wireFormat = format
        guard let inputConverter = AVAudioConverter(from: inputFormat, to: format) else {
            fail("could not create the microphone sample-rate converter")
        }
        // Voice Processing I/O exposes microphone plus echo-reference metadata
        // channels on recent macOS releases. Channel 0 is the processed mic.
        inputConverter.channelMap = [0]
        converter = inputConverter

        engine.attach(player)
        engine.connect(player, to: engine.mainMixerNode, format: format)
        engine.connect(engine.mainMixerNode, to: engine.outputNode, format: inputFormat)
        input.installTap(onBus: 0, bufferSize: 960, format: inputFormat) {
            [converter, wireFormat] buffer, _ in
            let ratio = wireFormat.sampleRate / inputFormat.sampleRate
            let capacity = AVAudioFrameCount(ceil(Double(buffer.frameLength) * ratio)) + 16
            guard let converted = AVAudioPCMBuffer(
                pcmFormat: wireFormat,
                frameCapacity: capacity
            ) else {
                return
            }
            var supplied = false
            var conversionError: NSError?
            let status = converter.convert(to: converted, error: &conversionError) {
                _, statusPointer in
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
            FileHandle.standardOutput.write(
                Data((pcm.base64EncodedString() + "\n").utf8)
            )
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
        guard let data = Data(base64Encoded: base64), !data.isEmpty else {
            return
        }
        guard data.count % MemoryLayout<Int16>.size == 0 else {
            return
        }
        let frames = AVAudioFrameCount(data.count / MemoryLayout<Int16>.size)
        guard let buffer = AVAudioPCMBuffer(pcmFormat: wireFormat, frameCapacity: frames),
              let destination = buffer.int16ChannelData?[0] else {
            return
        }
        _ = data.copyBytes(
            to: UnsafeMutableBufferPointer(start: destination, count: Int(frames))
        )
        buffer.frameLength = frames
        player.scheduleBuffer(buffer)
        if !player.isPlaying {
            player.play()
        }
    }

    func stopPlayback() {
        player.stop()
    }

    func close() {
        player.stop()
        engine.stop()
        engine.inputNode.removeTap(onBus: 0)
    }
}

// Commands arrive as newline-delimited records on stdin:
//   audio BASE64_PCM16_24KHZ_MONO
//   stop
// Keeping this control pipe and microphone output in one process lets the
// AVAudioEngine voice-processing I/O nodes perform real full-duplex AEC.
private func runDuplexAudio() {
    let device = DuplexAudioDevice()
    device.start()
    while let command = readLine(strippingNewline: true) {
        if command == "stop" {
            device.stopPlayback()
        } else if command.hasPrefix("audio ") {
            device.play(base64: String(command.dropFirst("audio ".count)))
        }
    }
    device.close()
}

private func play(path: String) {
    guard let data = FileManager.default.contents(atPath: path), !data.isEmpty else {
        fail("no audio data was written to \(path)")
    }
    guard data.count % MemoryLayout<Int16>.size == 0 else {
        fail("the PCM file has an incomplete 16-bit sample")
    }
    guard let format = AVAudioFormat(
        commonFormat: .pcmFormatInt16,
        sampleRate: sampleRate,
        channels: channels,
        interleaved: false
    ) else {
        fail("could not create the playback format")
    }
    let frames = AVAudioFrameCount(data.count / MemoryLayout<Int16>.size)
    guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frames),
          let destination = buffer.int16ChannelData?[0] else {
        fail("could not allocate the playback buffer")
    }
    _ = data.copyBytes(to: UnsafeMutableBufferPointer(start: destination, count: Int(frames)))
    buffer.frameLength = frames

    let engine = AVAudioEngine()
    let player = AVAudioPlayerNode()
    let completed = DispatchSemaphore(value: 0)
    engine.attach(player)
    engine.connect(player, to: engine.mainMixerNode, format: format)
    player.scheduleBuffer(buffer, completionCallbackType: .dataPlayedBack) { _ in
        completed.signal()
    }
    do {
        engine.prepare()
        try engine.start()
        player.play()
        completed.wait()
        player.stop()
        engine.stop()
    } catch {
        fail("could not play the response: \(error)")
    }
}

let arguments = CommandLine.arguments
guard arguments.count >= 2 else {
    fail("usage: realtime_audio_macos.swift duplex | stream | record|play PATH [MAX_SECONDS] [SILENCE_SECONDS]")
}

switch arguments[1] {
case "duplex":
    runDuplexAudio()
case "stream":
    streamMicrophone()
case "record":
    guard arguments.count >= 3 else {
        fail("record requires an output path")
    }
    let maxSeconds = arguments.count > 3 ? Double(arguments[3]) ?? 12 : 12
    let silenceSeconds = arguments.count > 4 ? Double(arguments[4]) ?? 1.25 : 1.25
    record(to: arguments[2], maximumSeconds: maxSeconds, silenceSeconds: silenceSeconds)
case "play":
    guard arguments.count >= 3 else {
        fail("play requires an input path")
    }
    play(path: arguments[2])
default:
    fail("unknown operation \(arguments[1]); expected duplex, stream, record, or play")
}
