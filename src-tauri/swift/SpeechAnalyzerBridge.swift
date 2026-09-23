// Apple SpeechAnalyzer bridge. Compiled by build.rs into a static library and called from
// engine/speechanalyzer.rs through the C ABI. SpeechAnalyzer has no C or Objective-C surface,
// which is why this file exists (docs/TECH-STACK.md#speech-recognition).
//
// Deployment target is macOS 15 so the binary loads on an older OS; every SpeechAnalyzer use
// sits behind `#available(macOS 26.0, *)` and reports unavailable instead of failing to launch.
//
// Also hosts the microphone permission query, because AVCaptureDevice is the only API for it.
//
// Transcripts are returned to Rust and never logged here.

import AVFoundation
import Foundation
import Speech

// MARK: - JSON plumbing

private struct PrepareReport: Codable {
    var ok = false
    var locale: String? = nil
    var assetStatus: String? = nil
    var downloadMs: Double? = nil
    var warmupMs: Double? = nil
    var error: String? = nil
}

private struct TranscribeReport: Codable {
    var text = ""
    var ms = 0.0
    var finals = 0
    var error: String? = nil
}

private func json<T: Codable>(_ v: T) -> UnsafeMutablePointer<CChar>? {
    let enc = JSONEncoder()
    enc.outputFormatting = [.sortedKeys]
    guard let data = try? enc.encode(v), let s = String(data: data, encoding: .utf8) else {
        return strdup("{\"error\":\"encode failed\"}")
    }
    return strdup(s)
}

private func ms(_ since: Date) -> Double { Date().timeIntervalSince(since) * 1000 }

/// Runs an async job to completion from a synchronous C entry point.
private final class Box<T>: @unchecked Sendable { var value: T; init(_ v: T) { value = v } }
private func blocking<T>(_ initial: T, _ job: @escaping @Sendable () async -> T) -> T {
    let sem = DispatchSemaphore(value: 0)
    let box = Box(initial)
    Task.detached(priority: .userInitiated) {
        box.value = await job()
        sem.signal()
    }
    sem.wait()
    return box.value
}

// MARK: - Engine state

@available(macOS 26.0, *)
private final class EngineState: @unchecked Sendable {
    static let shared = EngineState()
    var locale: Locale? = nil
}

private func resolveLocale(_ id: String) -> Locale {
    id.isEmpty || id == "auto" ? Locale.current : Locale(identifier: id)
}

// MARK: - C entry points

@_cdecl("vox_sa_available")
public func vox_sa_available() -> Bool {
    if #available(macOS 26.0, *) {
        return SpeechTranscriber.isAvailable
    }
    return false
}

/// Resolves the locale, installs Apple's model assets if needed (network, once), and warms
/// the analyzer. Returns a JSON PrepareReport. Free with vox_sa_free.
@_cdecl("vox_sa_prepare")
public func vox_sa_prepare(_ cLocale: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>? {
    let wanted = String(cString: cLocale)
    var rep = PrepareReport()
    guard #available(macOS 26.0, *) else {
        rep.error = "SpeechAnalyzer needs macOS 26; this is \(ProcessInfo.processInfo.operatingSystemVersionString)"
        return json(rep)
    }
    rep = blocking(rep) {
        var r = PrepareReport()
        guard let locale = await SpeechTranscriber.supportedLocale(equivalentTo: resolveLocale(wanted)) else {
            r.error = "locale \(wanted) is not supported by SpeechTranscriber"
            return r
        }
        r.locale = locale.identifier
        let probe = SpeechTranscriber(locale: locale, preset: .transcription)
        let status = await AssetInventory.status(forModules: [probe])
        r.assetStatus = "\(status)"
        if status != .installed {
            do {
                let t = Date()
                if let req = try await AssetInventory.assetInstallationRequest(supporting: [probe]) {
                    try await req.downloadAndInstall()
                }
                r.downloadMs = ms(t)
                r.assetStatus = "\(await AssetInventory.status(forModules: [probe]))"
            } catch {
                r.error = "model assets for \(locale.identifier) could not be installed: \(error)"
                return r
            }
        }
        // Warm-up: the first prepareToAnalyze in a process costs ~90–165 ms; with
        // .processLifetime retention every later dictation skips it (docs/spikes/s3-engine.md).
        do {
            let t = Date()
            let analyzer = SpeechAnalyzer(
                modules: [probe],
                options: .init(priority: .userInitiated, modelRetention: .processLifetime))
            let fmt = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [probe])
            try await analyzer.prepareToAnalyze(in: fmt)
            await analyzer.cancelAndFinishNow()
            r.warmupMs = ms(t)
        } catch {
            r.error = "warm-up failed: \(error)"
            return r
        }
        EngineState.shared.locale = locale
        r.ok = true
        return r
    }
    return json(rep)
}

/// Transcribes `len` float32 samples at `sampleRate` Hz, mono. Returns a JSON TranscribeReport.
@_cdecl("vox_sa_transcribe")
public func vox_sa_transcribe(
    _ pcm: UnsafePointer<Float>, _ len: Int, _ sampleRate: Double, _ cLocale: UnsafePointer<CChar>
) -> UnsafeMutablePointer<CChar>? {
    let wanted = String(cString: cLocale)
    var rep = TranscribeReport()
    guard #available(macOS 26.0, *) else {
        rep.error = "SpeechAnalyzer needs macOS 26"
        return json(rep)
    }
    // Copy the samples now; the Rust buffer is only valid for the duration of this call.
    let samples = Array(UnsafeBufferPointer(start: pcm, count: len))
    rep = blocking(rep) {
        var r = TranscribeReport()
        let t0 = Date()
        var resolved = EngineState.shared.locale
        if resolved == nil {
            resolved = await SpeechTranscriber.supportedLocale(equivalentTo: resolveLocale(wanted))
        }
        let locale = resolved ?? Locale(identifier: "en_US")
        do {
            let transcriber = SpeechTranscriber(locale: locale, preset: .transcription)
            let analyzer = SpeechAnalyzer(
                modules: [transcriber],
                options: .init(priority: .userInitiated, modelRetention: .processLifetime))
            let best = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber])
            try await analyzer.prepareToAnalyze(in: best)

            guard let inFmt = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: sampleRate, channels: 1, interleaved: false),
                  let inBuf = AVAudioPCMBuffer(pcmFormat: inFmt, frameCapacity: AVAudioFrameCount(max(samples.count, 1)))
            else {
                r.error = "could not allocate audio buffer"
                return r
            }
            inBuf.frameLength = AVAudioFrameCount(samples.count)
            samples.withUnsafeBufferPointer { src in
                inBuf.floatChannelData![0].update(from: src.baseAddress!, count: samples.count)
            }

            // The analyzer wants its own format (16 kHz Int16 mono on this machine); convert.
            let input: AVAudioPCMBuffer
            if let best, best.sampleRate != inFmt.sampleRate || best.commonFormat != inFmt.commonFormat
                || best.channelCount != inFmt.channelCount || best.isInterleaved != inFmt.isInterleaved
            {
                guard let conv = AVAudioConverter(from: inFmt, to: best) else {
                    r.error = "no converter from \(inFmt) to \(best)"
                    return r
                }
                let ratio = best.sampleRate / inFmt.sampleRate
                let cap = AVAudioFrameCount(Double(inBuf.frameLength) * ratio) + 1024
                guard let outBuf = AVAudioPCMBuffer(pcmFormat: best, frameCapacity: cap) else {
                    r.error = "could not allocate converted buffer"
                    return r
                }
                var consumed = false
                var convError: NSError? = nil
                let status = conv.convert(to: outBuf, error: &convError) { _, outStatus in
                    if consumed {
                        outStatus.pointee = .endOfStream
                        return nil
                    }
                    consumed = true
                    outStatus.pointee = .haveData
                    return inBuf
                }
                if status == .error {
                    r.error = "conversion failed: \(convError?.localizedDescription ?? "?")"
                    return r
                }
                input = outBuf
            } else {
                input = inBuf
            }

            let box = Box<(String, Int)>(("", 0))
            let results = Task {
                for try await res in transcriber.results where res.isFinal {
                    box.value.0 += String(res.text.characters)
                    box.value.1 += 1
                }
            }
            let (stream, cont) = AsyncStream.makeStream(of: AnalyzerInput.self)
            cont.yield(AnalyzerInput(buffer: input))
            cont.finish()
            let last = try await analyzer.analyzeSequence(stream)
            if let last {
                try await analyzer.finalizeAndFinish(through: last)
            } else {
                try await analyzer.finalizeAndFinishThroughEndOfInput()
            }
            try await results.value
            r.text = box.value.0.trimmingCharacters(in: .whitespacesAndNewlines)
            r.finals = box.value.1
        } catch {
            r.error = "\(error)"
        }
        r.ms = ms(t0)
        return r
    }
    return json(rep)
}

// MARK: - Streaming sessions
//
// The pipeline pushes 16 kHz mono float chunks while the key is held; the analyzer works on
// them as they arrive, so at release only the tail is left to finalise. Volatile results are
// requested to keep the model processing incrementally; only final results are kept.
// All entry points are called from the single Rust engine thread.

private struct StreamStartReport: Codable {
    var ok = false
    var handle: Int32 = 0
    var error: String? = nil
}

private struct StreamFinishReport: Codable {
    var text = ""
    var ms = 0.0
    var finals = 0
    var volatiles = 0
    var pushedSeconds = 0.0
    var error: String? = nil
}

@available(macOS 26.0, *)
private final class StreamSession: @unchecked Sendable {
    let transcriber: SpeechTranscriber
    let analyzer: SpeechAnalyzer
    let cont: AsyncStream<AnalyzerInput>.Continuation
    let inFmt: AVAudioFormat
    let outFmt: AVAudioFormat?
    let converter: AVAudioConverter?
    let box = Box<(String, Int, Int)>(("", 0, 0))
    var results: Task<Void, Error>? = nil
    var pushedFrames = 0

    init(transcriber: SpeechTranscriber, analyzer: SpeechAnalyzer,
         cont: AsyncStream<AnalyzerInput>.Continuation, inFmt: AVAudioFormat, outFmt: AVAudioFormat?) {
        self.transcriber = transcriber
        self.analyzer = analyzer
        self.cont = cont
        self.inFmt = inFmt
        self.outFmt = outFmt
        if let outFmt, outFmt != inFmt {
            self.converter = AVAudioConverter(from: inFmt, to: outFmt)
        } else {
            self.converter = nil
        }
    }
}

@available(macOS 26.0, *)
private final class StreamRegistry: @unchecked Sendable {
    static let shared = StreamRegistry()
    var sessions: [Int32: StreamSession] = [:]
    var next: Int32 = 1
}

@_cdecl("vox_sa_stream_start")
public func vox_sa_stream_start(_ cLocale: UnsafePointer<CChar>, _ sampleRate: Double) -> UnsafeMutablePointer<CChar>? {
    let wanted = String(cString: cLocale)
    var rep = StreamStartReport()
    guard #available(macOS 26.0, *) else {
        rep.error = "SpeechAnalyzer needs macOS 26"
        return json(rep)
    }
    rep = blocking(rep) {
        var r = StreamStartReport()
        var resolved = EngineState.shared.locale
        if resolved == nil {
            resolved = await SpeechTranscriber.supportedLocale(equivalentTo: resolveLocale(wanted))
        }
        let locale = resolved ?? Locale(identifier: "en_US")
        do {
            let transcriber = SpeechTranscriber(
                locale: locale,
                transcriptionOptions: [],
                reportingOptions: [.volatileResults],
                attributeOptions: [])
            let analyzer = SpeechAnalyzer(
                modules: [transcriber],
                options: .init(priority: .userInitiated, modelRetention: .processLifetime))
            let best = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber])
            try await analyzer.prepareToAnalyze(in: best)
            guard let inFmt = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: sampleRate, channels: 1, interleaved: false) else {
                r.error = "bad input format"
                return r
            }
            let (stream, cont) = AsyncStream.makeStream(of: AnalyzerInput.self)
            let session = StreamSession(transcriber: transcriber, analyzer: analyzer, cont: cont, inFmt: inFmt, outFmt: best)
            let box = session.box
            session.results = Task {
                for try await res in transcriber.results {
                    if res.isFinal {
                        box.value.0 += String(res.text.characters)
                        box.value.1 += 1
                    } else {
                        box.value.2 += 1
                    }
                }
            }
            try await analyzer.start(inputSequence: stream)
            let reg = StreamRegistry.shared
            let handle = reg.next
            reg.next += 1
            reg.sessions[handle] = session
            r.handle = handle
            r.ok = true
        } catch {
            r.error = "\(error)"
        }
        return r
    }
    return json(rep)
}

/// Pushes `len` float32 samples at the session's input rate. Returns false if the handle is
/// unknown or the conversion failed.
@_cdecl("vox_sa_stream_push")
public func vox_sa_stream_push(_ handle: Int32, _ pcm: UnsafePointer<Float>, _ len: Int) -> Bool {
    guard #available(macOS 26.0, *), len > 0,
          let s = StreamRegistry.shared.sessions[handle] else { return false }
    guard let inBuf = AVAudioPCMBuffer(pcmFormat: s.inFmt, frameCapacity: AVAudioFrameCount(len)) else { return false }
    inBuf.frameLength = AVAudioFrameCount(len)
    inBuf.floatChannelData![0].update(from: pcm, count: len)
    s.pushedFrames += len
    if let conv = s.converter, let outFmt = s.outFmt {
        let ratio = outFmt.sampleRate / s.inFmt.sampleRate
        let cap = AVAudioFrameCount(Double(len) * ratio) + 64
        guard let outBuf = AVAudioPCMBuffer(pcmFormat: outFmt, frameCapacity: cap) else { return false }
        var consumed = false
        var err: NSError? = nil
        let status = conv.convert(to: outBuf, error: &err) { _, outStatus in
            if consumed {
                outStatus.pointee = .noDataNow
                return nil
            }
            consumed = true
            outStatus.pointee = .haveData
            return inBuf
        }
        if status == .error { return false }
        if outBuf.frameLength > 0 { s.cont.yield(AnalyzerInput(buffer: outBuf)) }
    } else {
        s.cont.yield(AnalyzerInput(buffer: inBuf))
    }
    return true
}

/// Ends input, finalises whatever is left, and returns the text. The session is removed.
@_cdecl("vox_sa_stream_finish")
public func vox_sa_stream_finish(_ handle: Int32) -> UnsafeMutablePointer<CChar>? {
    var rep = StreamFinishReport()
    guard #available(macOS 26.0, *) else {
        rep.error = "SpeechAnalyzer needs macOS 26"
        return json(rep)
    }
    guard let s = StreamRegistry.shared.sessions.removeValue(forKey: handle) else {
        rep.error = "unknown stream handle \(handle)"
        return json(rep)
    }
    rep = blocking(rep) {
        var r = StreamFinishReport()
        let t0 = Date()
        r.pushedSeconds = Double(s.pushedFrames) / s.inFmt.sampleRate
        do {
            s.cont.finish()
            try await s.analyzer.finalizeAndFinishThroughEndOfInput()
            try await s.results?.value
            r.text = s.box.value.0.trimmingCharacters(in: .whitespacesAndNewlines)
            r.finals = s.box.value.1
            r.volatiles = s.box.value.2
        } catch {
            r.error = "\(error)"
        }
        r.ms = ms(t0)
        return r
    }
    return json(rep)
}

@_cdecl("vox_sa_stream_cancel")
public func vox_sa_stream_cancel(_ handle: Int32) {
    guard #available(macOS 26.0, *),
          let s = StreamRegistry.shared.sessions.removeValue(forKey: handle) else { return }
    _ = blocking(false) {
        s.cont.finish()
        await s.analyzer.cancelAndFinishNow()
        s.results?.cancel()
        return true
    }
}

@_cdecl("vox_sa_free")
public func vox_sa_free(_ p: UnsafeMutablePointer<CChar>?) { free(p) }

// MARK: - Microphone permission (AVCaptureDevice is the only API for it)

/// 0 not determined · 1 restricted · 2 denied · 3 authorized (AVAuthorizationStatus raw values).
@_cdecl("vox_mic_status")
public func vox_mic_status() -> Int32 {
    Int32(AVCaptureDevice.authorizationStatus(for: .audio).rawValue)
}

/// Shows the system prompt if not yet determined; blocks until answered. Returns granted.
@_cdecl("vox_mic_request")
public func vox_mic_request() -> Bool {
    let sem = DispatchSemaphore(value: 0)
    let box = Box(false)
    AVCaptureDevice.requestAccess(for: .audio) { granted in
        box.value = granted
        sem.signal()
    }
    sem.wait()
    return box.value
}
