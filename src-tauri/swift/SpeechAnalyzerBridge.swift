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
import CoreMedia
import Foundation
import Speech
import UserNotifications

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
    var volatiles = 0
    var usedVolatileTail = false
    var module: String? = nil
    var hintsDropped = false
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
    /// The dictation module's assets are installed and it has been warmed. Set by
    /// vox_sa_prepare_dictation's background task; read at stream start without waiting.
    var dictationReady = false
    var dictationPreparing = false
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

/// Transcribes `len` float32 samples at `sampleRate` Hz, mono, in one pass. `cContext` is
/// the recognition hints, one per line, empty for none: with hints and a ready dictation
/// module the pass runs on DictationTranscriber, otherwise on SpeechTranscriber (the batch
/// path is the fallback when streaming could not start, and the release-time comparison
/// in the engine's ignored tests). Returns a JSON TranscribeReport.
@_cdecl("vox_sa_transcribe")
public func vox_sa_transcribe(
    _ pcm: UnsafePointer<Float>, _ len: Int, _ sampleRate: Double, _ cLocale: UnsafePointer<CChar>,
    _ cContext: UnsafePointer<CChar>
) -> UnsafeMutablePointer<CChar>? {
    let wanted = String(cString: cLocale)
    let terms = String(cString: cContext).split(separator: "\n").map(String.init).filter { !$0.isEmpty }
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
        let useDictation = !terms.isEmpty && EngineState.shared.dictationReady
        r.hintsDropped = !terms.isEmpty && !useDictation
        do {
            let modules: [any SpeechModule]
            let box = Box<StreamText>(StreamText())
            let results: Task<Void, Error>
            if useDictation {
                let transcriber = dictationModule(locale)
                modules = [transcriber]
                results = Task {
                    for try await res in transcriber.results {
                        box.value.note(text: String(res.text.characters), isFinal: res.isFinal, range: res.range)
                    }
                }
                r.module = "dictation"
            } else {
                let transcriber = SpeechTranscriber(locale: locale, preset: .transcription)
                modules = [transcriber]
                results = Task {
                    for try await res in transcriber.results {
                        box.value.note(text: String(res.text.characters), isFinal: res.isFinal, range: res.range)
                    }
                }
                r.module = "speech"
            }
            let analyzer = SpeechAnalyzer(
                modules: modules,
                options: .init(priority: .userInitiated, modelRetention: .processLifetime))
            if useDictation {
                let ctx = AnalysisContext()
                ctx.contextualStrings = [.general: terms]
                try await analyzer.setContext(ctx)
            }
            let best = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: modules)
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
            let (text, usedTail) = box.value.merged()
            r.text = text.trimmingCharacters(in: .whitespacesAndNewlines)
            r.finals = box.value.finalCount
            r.volatiles = box.value.volatileCount
            r.usedVolatileTail = usedTail
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
// requested to keep the model processing incrementally; final results are kept, and a
// trailing volatile result the module never finalised is used as the tail.
// All entry points are called from the single Rust engine thread.
//
// Two modules of the same framework (docs/CONTEXT.md, docs/spikes/s5-context.md):
// SpeechTranscriber, today's engine, when there are no hints; DictationTranscriber, which
// honours AnalysisContext.contextualStrings, whenever hints exist and its assets are ready.
// Hints are per session, in memory, and never logged from here.

private struct StreamStartReport: Codable {
    var ok = false
    var handle: Int32 = 0
    var module: String? = nil
    var hintsDropped = false
    var error: String? = nil
}

private struct StreamFinishReport: Codable {
    var text = ""
    var ms = 0.0
    var finals = 0
    var volatiles = 0
    var usedVolatileTail = false
    var pushedSeconds = 0.0
    var error: String? = nil
}

/// What the results task has collected so far.
private struct StreamText {
    var finals = ""
    var finalCount = 0
    var volatileCount = 0
    var lastVolatile = ""
    var lastVolatileStart = CMTime.zero
    var lastFinalEnd = CMTime.zero

    mutating func note(text: String, isFinal: Bool, range: CMTimeRange) {
        if isFinal {
            finals += text
            finalCount += 1
            lastFinalEnd = range.end
            lastVolatile = ""
        } else {
            volatileCount += 1
            lastVolatile = text
            lastVolatileStart = range.start
        }
    }

    /// Finals, plus the last volatile result if it covers audio after the last final.
    func merged() -> (String, Bool) {
        let tailUsable = !lastVolatile.isEmpty && (finalCount == 0 || lastVolatileStart >= lastFinalEnd)
        return (finals + (tailUsable ? lastVolatile : ""), tailUsable)
    }
}

@available(macOS 26.0, *)
private final class StreamSession: @unchecked Sendable {
    let analyzer: SpeechAnalyzer
    let cont: AsyncStream<AnalyzerInput>.Continuation
    let inFmt: AVAudioFormat
    let outFmt: AVAudioFormat?
    let converter: AVAudioConverter?
    let module: String
    let box = Box<StreamText>(StreamText())
    var results: Task<Void, Error>? = nil
    var pushedFrames = 0
    /// Frames already covered by a periodic finalize request.
    var finalizedFrames = 0

    init(analyzer: SpeechAnalyzer, cont: AsyncStream<AnalyzerInput>.Continuation,
         inFmt: AVAudioFormat, outFmt: AVAudioFormat?, module: String) {
        self.analyzer = analyzer
        self.cont = cont
        self.inFmt = inFmt
        self.outFmt = outFmt
        self.module = module
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

@available(macOS 26.0, *)
private func dictationModule(_ locale: Locale) -> DictationTranscriber {
    DictationTranscriber(
        locale: locale,
        contentHints: [.shortForm],
        transcriptionOptions: [.punctuation],
        reportingOptions: [.volatileResults],
        attributeOptions: [])
}

/// Installs the dictation module's assets (separate from the speech module's) and warms it,
/// on a detached task so nothing on the dictation path waits. Called when the context
/// setting is on. Idempotent.
@_cdecl("vox_sa_prepare_dictation")
public func vox_sa_prepare_dictation() {
    guard #available(macOS 26.0, *) else { return }
    let state = EngineState.shared
    if state.dictationReady || state.dictationPreparing { return }
    state.dictationPreparing = true
    Task.detached(priority: .utility) {
        let t = Date()
        let locale = state.locale ?? Locale(identifier: "en_US")
        let probe = dictationModule(locale)
        do {
            var status = await AssetInventory.status(forModules: [probe])
            if status != .installed {
                if let req = try await AssetInventory.assetInstallationRequest(supporting: [probe]) {
                    try await req.downloadAndInstall()
                }
                status = await AssetInventory.status(forModules: [probe])
            }
            guard status == .installed else {
                blog("dictation module assets not installed (\(status)); hints will be dropped")
                state.dictationPreparing = false
                return
            }
            let analyzer = SpeechAnalyzer(
                modules: [probe],
                options: .init(priority: .userInitiated, modelRetention: .processLifetime))
            let fmt = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [probe])
            try await analyzer.prepareToAnalyze(in: fmt)
            await analyzer.cancelAndFinishNow()
            state.dictationReady = true
            blog("dictation module ready in \(Int(ms(t))) ms")
        } catch {
            blog("dictation module could not be prepared: \(error.localizedDescription)")
        }
        state.dictationPreparing = false
    }
}

@_cdecl("vox_sa_dictation_ready")
public func vox_sa_dictation_ready() -> Bool {
    guard #available(macOS 26.0, *) else { return false }
    return EngineState.shared.dictationReady
}

/// Starts a streaming session. `cContext` is the recognition hints, one per line, empty for
/// none. With hints and a ready dictation module the session runs on DictationTranscriber
/// with the hints as contextual strings; otherwise on SpeechTranscriber, and the report says
/// the hints were dropped.
@_cdecl("vox_sa_stream_start")
public func vox_sa_stream_start(_ cLocale: UnsafePointer<CChar>, _ sampleRate: Double,
                                _ cContext: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>? {
    let wanted = String(cString: cLocale)
    let terms = String(cString: cContext).split(separator: "\n").map(String.init).filter { !$0.isEmpty }
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
        let useDictation = !terms.isEmpty && EngineState.shared.dictationReady
        r.hintsDropped = !terms.isEmpty && !useDictation
        do {
            guard let inFmt = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: sampleRate, channels: 1, interleaved: false) else {
                r.error = "bad input format"
                return r
            }
            let (stream, cont) = AsyncStream.makeStream(of: AnalyzerInput.self)
            let session: StreamSession
            if useDictation {
                let transcriber = dictationModule(locale)
                let analyzer = SpeechAnalyzer(
                    modules: [transcriber],
                    options: .init(priority: .userInitiated, modelRetention: .processLifetime))
                let ctx = AnalysisContext()
                ctx.contextualStrings = [.general: terms]
                try await analyzer.setContext(ctx)
                let best = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber])
                try await analyzer.prepareToAnalyze(in: best)
                session = StreamSession(analyzer: analyzer, cont: cont, inFmt: inFmt, outFmt: best, module: "dictation")
                let box = session.box
                session.results = Task {
                    for try await res in transcriber.results {
                        box.value.note(text: String(res.text.characters), isFinal: res.isFinal, range: res.range)
                    }
                }
            } else {
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
                session = StreamSession(analyzer: analyzer, cont: cont, inFmt: inFmt, outFmt: best, module: "speech")
                let box = session.box
                session.results = Task {
                    for try await res in transcriber.results {
                        box.value.note(text: String(res.text.characters), isFinal: res.isFinal, range: res.range)
                    }
                }
            }
            try await session.analyzer.start(inputSequence: stream)
            let reg = StreamRegistry.shared
            let handle = reg.next
            reg.next += 1
            reg.sessions[handle] = session
            r.handle = handle
            r.module = session.module
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
    // Commit earlier audio while the user is still talking, so the work left at release is
    // bounded by the lag, not by the length of the utterance. Every ~4 s of audio, ask for
    // everything older than ~1.5 s to be finalised; the request is asynchronous.
    let commitEvery = Int(s.inFmt.sampleRate * 4)
    let keepBack = s.inFmt.sampleRate * 1.5
    if s.pushedFrames - s.finalizedFrames >= commitEvery {
        s.finalizedFrames = s.pushedFrames
        let through = CMTime(seconds: Double(s.pushedFrames) / s.inFmt.sampleRate - keepBack, preferredTimescale: 16_000)
        let analyzer = s.analyzer
        Task.detached(priority: .userInitiated) {
            try? await analyzer.finalize(through: through)
        }
    }
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
            let (text, usedTail) = s.box.value.merged()
            r.text = text.trimmingCharacters(in: .whitespacesAndNewlines)
            r.finals = s.box.value.finalCount
            r.volatiles = s.box.value.volatileCount
            r.usedVolatileTail = usedTail
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

// MARK: - Bridge logging

/// Writes into Vox's own log through Rust's tracing (src-tauri/src/lib.rs), so bridge
/// diagnostics land next to the pipeline's and are not redacted as `<private>` the way NSLog
/// arguments are in the unified log. Never called with transcript text.
@_silgen_name("vox_bridge_log")
private func vox_bridge_log(_ message: UnsafePointer<CChar>)

private func blog(_ message: String) {
    message.withCString { vox_bridge_log($0) }
}

// MARK: - Notifications

/// Posts a user notification through UserNotifications. tauri-plugin-notification goes through
/// the deprecated NSUserNotification API, which current macOS drops silently: the app never
/// even appears in System Settings → Notifications. Authorization is requested on first use,
/// so the system prompt appears with the first failure notification, not at launch.
///
/// Returns false when the process is not running from an .app bundle (a bare `cargo run`),
/// where UNUserNotificationCenter would abort; the caller falls back to the plugin.
/// Bodies are fixed copy-deck strings; nothing here logs them.
@_cdecl("vox_notify")
public func vox_notify(_ cTitle: UnsafePointer<CChar>, _ cBody: UnsafePointer<CChar>) -> Bool {
    guard Bundle.main.bundleURL.pathExtension == "app", Bundle.main.bundleIdentifier != nil else {
        blog("notify: not running from an app bundle (\(Bundle.main.bundleURL.path))")
        return false
    }
    let title = String(cString: cTitle)
    let body = String(cString: cBody)
    let center = UNUserNotificationCenter.current()
    let deliver: @Sendable () -> Void = {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        let request = UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil)
        center.add(request) { error in
            if let error {
                blog("notification not delivered: \(error.localizedDescription)")
            } else {
                blog("notification posted")
            }
        }
    }
    center.getNotificationSettings { settings in
        let status = settings.authorizationStatus.rawValue
        blog("notify: authorization status \(status) (0 not determined, 1 denied, 2 authorized)")
        switch settings.authorizationStatus {
        case .notDetermined:
            center.requestAuthorization(options: [.alert, .sound]) { granted, error in
                if granted {
                    deliver()
                } else {
                    blog("notifications not authorised: \(error?.localizedDescription ?? "declined")")
                }
            }
        case .denied:
            blog("notifications denied: turned off in System Settings, or the app signature is not trusted by Notification Center")
        default:
            deliver()
        }
    }
    return true
}

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
