// S3 bridge: run Apple SpeechAnalyzer over an audio file and report as JSON.
// Called from Rust through the C ABI. Compiled by build.rs with deployment target macOS 15,
// so every SpeechAnalyzer use sits behind `#available(macOS 26.0, *)`.
import Foundation
import AVFoundation
import Speech

struct RunReport: Codable {
    var index = 0
    var prepareMs = 0.0
    var firstResultMs: Double? = nil
    var analyzeMs = 0.0
    var finalizeMs = 0.0
    var totalMs = 0.0
    var volatileResults = 0
    var finalResults = 0
    var text = ""
    var error: String? = nil
}

struct Report: Codable {
    var os = ProcessInfo.processInfo.operatingSystemVersionString
    var frameworkAvailable = false
    var transcriberIsAvailable: Bool? = nil
    var supportedLocaleCount: Int? = nil
    var installedLocales: [String]? = nil
    var requestedLocale = ""
    var resolvedLocale: String? = nil
    var assetStatusBefore: String? = nil
    var assetDownloadMs: Double? = nil
    var assetStatusAfter: String? = nil
    var reservedLocales: [String]? = nil
    var audioSeconds: Double? = nil
    var audioFormat: String? = nil
    var bestAnalyzerFormat: String? = nil
    var runs: [RunReport] = []
    var peakRssMB = 0.0
    var error: String? = nil
}

private func ms(_ since: Date) -> Double { Date().timeIntervalSince(since) * 1000 }

private func peakRssMB() -> Double {
    var ru = rusage()
    getrusage(RUSAGE_SELF, &ru)
    return Double(ru.ru_maxrss) / 1_048_576  // bytes on macOS
}

private func describe(_ f: AVAudioFormat?) -> String {
    guard let f else { return "nil" }
    return "\(Int(f.sampleRate)) Hz, \(f.channelCount) ch, \(f.commonFormat.rawValue == 1 ? "float32" : "fmt \(f.commonFormat.rawValue)")\(f.isInterleaved ? " interleaved" : "")"
}

@available(macOS 26.0, *)
private func runOnce(index: Int, url: URL, locale: Locale, report: inout Report) async -> RunReport {
    var r = RunReport(); r.index = index
    do {
        let t0 = Date()
        let transcriber = SpeechTranscriber(locale: locale, preset: .transcription)
        let analyzer = SpeechAnalyzer(
            modules: [transcriber],
            options: .init(priority: .userInitiated, modelRetention: .processLifetime))
        let fmt = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber])
        if report.bestAnalyzerFormat == nil { report.bestAnalyzerFormat = describe(fmt) }
        let tPrep = Date()
        try await analyzer.prepareToAnalyze(in: fmt)
        r.prepareMs = ms(tPrep)

        let file = try AVAudioFile(forReading: url)
        let tStart = Date()
        let box = ResultBox()
        let results = Task {
            for try await res in transcriber.results {
                box.record(res, since: tStart)
            }
        }
        let last = try await analyzer.analyzeSequence(from: file)
        r.analyzeMs = ms(tStart)
        let tFin = Date()
        if let last { try await analyzer.finalizeAndFinish(through: last) }
        else { try await analyzer.finalizeAndFinishThroughEndOfInput() }
        try await results.value
        r.finalizeMs = ms(tFin)
        r.totalMs = ms(tStart)
        r.firstResultMs = box.firstMs
        r.volatileResults = box.volatile
        r.finalResults = box.finals
        r.text = box.text.trimmingCharacters(in: .whitespacesAndNewlines)
        _ = t0
    } catch {
        r.error = "\(error)"
    }
    return r
}

@available(macOS 26.0, *)
private final class ResultBox: @unchecked Sendable {
    var firstMs: Double? = nil
    var volatile = 0
    var finals = 0
    var text = ""
    func record(_ res: SpeechTranscriber.Result, since: Date) {
        if firstMs == nil { firstMs = ms(since) }
        if res.isFinal { finals += 1; text += String(res.text.characters) } else { volatile += 1 }
    }
}

private func run(path: String, localeId: String, runs: Int, download: Bool) async -> Report {
    var rep = Report()
    rep.requestedLocale = localeId
    guard #available(macOS 26.0, *) else {
        rep.error = "SpeechAnalyzer needs macOS 26; this is \(rep.os)"
        rep.peakRssMB = peakRssMB()
        return rep
    }
    rep.frameworkAvailable = true
    rep.transcriberIsAvailable = SpeechTranscriber.isAvailable
    rep.supportedLocaleCount = await SpeechTranscriber.supportedLocales.count
    rep.installedLocales = await SpeechTranscriber.installedLocales.map { $0.identifier }
    guard let locale = await SpeechTranscriber.supportedLocale(equivalentTo: Locale(identifier: localeId)) else {
        rep.error = "locale \(localeId) not supported by SpeechTranscriber"
        rep.peakRssMB = peakRssMB()
        return rep
    }
    rep.resolvedLocale = locale.identifier

    let probe = SpeechTranscriber(locale: locale, preset: .transcription)
    let status = await AssetInventory.status(forModules: [probe])
    rep.assetStatusBefore = "\(status)"
    if status != .installed {
        if download {
            do {
                let t = Date()
                if let req = try await AssetInventory.assetInstallationRequest(supporting: [probe]) {
                    try await req.downloadAndInstall()
                }
                rep.assetDownloadMs = ms(t)
                rep.assetStatusAfter = "\(await AssetInventory.status(forModules: [probe]))"
            } catch {
                rep.error = "asset download failed: \(error)"
                rep.peakRssMB = peakRssMB()
                return rep
            }
        } else {
            rep.error = "assets for \(locale.identifier) are \(status), not installed; re-run with --download"
            rep.peakRssMB = peakRssMB()
            return rep
        }
    }
    rep.reservedLocales = await AssetInventory.reservedLocales.map { $0.identifier }

    let url = URL(fileURLWithPath: path)
    do {
        let file = try AVAudioFile(forReading: url)
        rep.audioSeconds = Double(file.length) / file.fileFormat.sampleRate
        rep.audioFormat = describe(file.fileFormat)
    } catch {
        rep.error = "cannot open audio: \(error)"
        rep.peakRssMB = peakRssMB()
        return rep
    }

    for i in 0..<max(runs, 1) {
        rep.runs.append(await runOnce(index: i, url: url, locale: locale, report: &rep))
    }
    rep.peakRssMB = peakRssMB()
    return rep
}

private final class OutBox: @unchecked Sendable { var json = "{}" }

@_cdecl("vox_s3_run")
public func vox_s3_run(_ cPath: UnsafePointer<CChar>, _ cLocale: UnsafePointer<CChar>, _ runs: Int32, _ download: Bool) -> UnsafeMutablePointer<CChar>? {
    let path = String(cString: cPath)
    let localeId = String(cString: cLocale)
    let sem = DispatchSemaphore(value: 0)
    let out = OutBox()
    Task.detached {
        let rep = await run(path: path, localeId: localeId, runs: Int(runs), download: download)
        let enc = JSONEncoder(); enc.outputFormatting = [.sortedKeys]
        if let data = try? enc.encode(rep), let s = String(data: data, encoding: .utf8) { out.json = s }
        sem.signal()
    }
    sem.wait()
    return strdup(out.json)
}

@_cdecl("vox_s3_free")
public func vox_s3_free(_ p: UnsafeMutablePointer<CChar>?) { free(p) }
