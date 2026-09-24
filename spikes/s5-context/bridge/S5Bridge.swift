// S5 bridge: transcribe a file under several biasing conditions and report each.
//
//   st          SpeechTranscriber, no context (baseline)
//   st+ctx      SpeechTranscriber, AnalysisContext.contextualStrings passed at analyzer creation
//   dt          DictationTranscriber(.shortDictation), no context
//   dt+ctx      DictationTranscriber with contextualStrings
//   dt+lm       DictationTranscriber with a custom language model compiled from the terms
//               (SFCustomLanguageModelData → SFSpeechLanguageModel.prepareCustomLanguageModel →
//               ContentHint.customizedLanguage), the successor of SFCustomLanguageModel.
//
// Called from Rust through the C ABI.
import AVFoundation
import Foundation
import Speech

private struct Run: Codable {
    var text = ""
    var ms = 0.0
    var prepMs: Double? = nil
    var finals = 0
    var volatiles = 0
    var assets: String? = nil
    var error: String? = nil
}

private struct Report: Codable {
    var available = false
    var locale: String? = nil
    var contextTerms: [String] = []
    var order: [String] = []
    var runs: [String: Run] = [:]
    var error: String? = nil
}

private final class Box<T>: @unchecked Sendable { var value: T; init(_ v: T) { value = v } }
private func blocking<T>(_ initial: T, _ job: @escaping @Sendable () async -> T) -> T {
    let sem = DispatchSemaphore(value: 0)
    let box = Box(initial)
    Task.detached(priority: .userInitiated) { box.value = await job(); sem.signal() }
    sem.wait()
    return box.value
}
private func ms(_ since: Date) -> Double { Date().timeIntervalSince(since) * 1000 }

@available(macOS 26.0, *)
private func context(_ terms: [String]) -> AnalysisContext {
    let ctx = AnalysisContext()
    if !terms.isEmpty { ctx.contextualStrings = [.general: terms] }
    return ctx
}

@available(macOS 26.0, *)
private func runST(url: URL, locale: Locale, terms: [String]) async -> Run {
    var r = Run()
    let t0 = Date()
    do {
        let transcriber = SpeechTranscriber(locale: locale, preset: .transcription)
        let file = try AVAudioFile(forReading: url)
        let box = Box("")
        let results = Task {
            for try await res in transcriber.results where res.isFinal { box.value += String(res.text.characters) }
        }
        // Context passed at creation; this initializer starts analysis of the file.
        let analyzer = try await SpeechAnalyzer(
            inputAudioFile: file, modules: [transcriber],
            options: .init(priority: .userInitiated, modelRetention: .processLifetime),
            analysisContext: context(terms), finishAfterFile: true)
        try await analyzer.finalizeAndFinishThroughEndOfInput()
        try await results.value
        r.text = box.value.trimmingCharacters(in: .whitespacesAndNewlines)
    } catch { r.error = "\(error)" }
    r.ms = ms(t0)
    return r
}

@available(macOS 26.0, *)
private func runDT(url: URL, locale: Locale, terms: [String], hints: Set<DictationTranscriber.ContentHint>) async -> Run {
    var r = Run()
    let t0 = Date()
    do {
        let transcriber = DictationTranscriber(
            locale: locale, contentHints: hints,
            transcriptionOptions: [.punctuation], reportingOptions: [], attributeOptions: [])
        // DictationTranscriber has its own assets, separate from SpeechTranscriber's.
        var status = await AssetInventory.status(forModules: [transcriber])
        if status != .installed {
            if let req = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
                try await req.downloadAndInstall()
            }
            status = await AssetInventory.status(forModules: [transcriber])
        }
        r.assets = "\(status)"
        let file = try AVAudioFile(forReading: url)
        let box = Box("")
        let lastVolatile = Box("")
        let counts = Box((0, 0))
        let results = Task {
            for try await res in transcriber.results {
                if res.isFinal {
                    box.value += String(res.text.characters); counts.value.0 += 1
                } else {
                    lastVolatile.value = String(res.text.characters); counts.value.1 += 1
                }
            }
        }
        let analyzer = try await SpeechAnalyzer(
            inputAudioFile: file, modules: [transcriber],
            options: .init(priority: .userInitiated, modelRetention: .processLifetime),
            analysisContext: context(terms), finishAfterFile: true)
        try await analyzer.finalizeAndFinishThroughEndOfInput()
        try await results.value
        // DictationTranscriber may only ever report a volatile result for a short file; use it.
        let text = box.value.isEmpty ? lastVolatile.value : box.value
        r.text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        r.finals = counts.value.0
        r.volatiles = counts.value.1
    } catch { r.error = "\(error)" }
    r.ms = ms(t0)
    return r
}

/// Builds and compiles a custom language model from the terms. Returns the configuration and
/// how long compilation took; the first compile on a machine can take a while.
@available(macOS 26.0, *)
private func customLM(locale: Locale, terms: [String]) async throws -> (SFSpeechLanguageModel.Configuration, Double) {
    let t0 = Date()
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent("vox-s5-lm", isDirectory: true)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    let data = SFCustomLanguageModelData(locale: locale, identifier: "com.pixelsmashing.dictation.s5", version: "1") {
        for t in terms { SFCustomLanguageModelData.PhraseCount(phrase: t, count: 50) }
    }
    let asset = dir.appendingPathComponent("terms.bin")
    try await data.export(to: asset)
    let compiled = dir.appendingPathComponent("terms.lm")
    let cfg = SFSpeechLanguageModel.Configuration(languageModel: compiled, vocabulary: nil)
    try await withCheckedThrowingContinuation { (c: CheckedContinuation<Void, Error>) in
        SFSpeechLanguageModel.prepareCustomLanguageModel(for: asset, configuration: cfg) { err in
            if let err { c.resume(throwing: err) } else { c.resume() }
        }
    }
    return (cfg, ms(t0))
}

@_cdecl("vox_s5_run")
public func vox_s5_run(_ cPath: UnsafePointer<CChar>, _ cLocale: UnsafePointer<CChar>, _ cContext: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>? {
    let path = String(cString: cPath)
    let localeId = String(cString: cLocale)
    let terms = String(cString: cContext).split(separator: "\n").map(String.init).filter { !$0.isEmpty }
    var rep = Report()
    rep.contextTerms = terms
    let enc = JSONEncoder(); enc.outputFormatting = [.sortedKeys]
    guard #available(macOS 26.0, *) else {
        rep.error = "needs macOS 26"
        return strdup((try? String(data: enc.encode(rep), encoding: .utf8)) ?? "{}")
    }
    rep = blocking(rep) {
        var r = rep
        r.available = SpeechTranscriber.isAvailable
        let wanted = localeId.isEmpty || localeId == "auto" ? Locale.current : Locale(identifier: localeId)
        guard let locale = await SpeechTranscriber.supportedLocale(equivalentTo: wanted) else {
            r.error = "locale unsupported"
            return r
        }
        r.locale = locale.identifier
        let url = URL(fileURLWithPath: path)
        _ = await runST(url: url, locale: locale, terms: [])  // warm
        r.order = ["st", "st+ctx", "dt", "dt+ctx", "dt+lm"]
        r.runs["st"] = await runST(url: url, locale: locale, terms: [])
        r.runs["st+ctx"] = await runST(url: url, locale: locale, terms: terms)
        r.runs["dt"] = await runDT(url: url, locale: locale, terms: [], hints: [.shortForm])
        r.runs["dt+ctx"] = await runDT(url: url, locale: locale, terms: terms, hints: [.shortForm])
        do {
            let (cfg, prep) = try await customLM(locale: locale, terms: terms)
            var run = await runDT(url: url, locale: locale, terms: [], hints: [.shortForm, .customizedLanguage(modelConfiguration: cfg)])
            run.prepMs = prep
            r.runs["dt+lm"] = run
        } catch {
            r.runs["dt+lm"] = Run(text: "", ms: 0, prepMs: nil, error: "custom LM: \(error)")
        }
        return r
    }
    return strdup((try? String(data: enc.encode(rep), encoding: .utf8)) ?? "{}")
}

@_cdecl("vox_s5_free")
public func vox_s5_free(_ p: UnsafeMutablePointer<CChar>?) { free(p) }
