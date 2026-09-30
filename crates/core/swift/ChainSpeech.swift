// SpeechAnalyzer bridge for crates/core/src/speech.rs — see
// agent-docs/capabilities/speech/research/MACOS.md. SpeechAnalyzer is a
// Swift-concurrency API with no Objective-C surface, so objc2 can't reach
// it; this file is compiled by build.rs into a static library and exposes
// plain C functions. Everything crossing the boundary is a C string or a
// number — JSON for structured data — never a Swift or Foundation object.

import AVFoundation
import Foundation
import Speech

public typealias ChainSpeechProgress = @convention(c) (UnsafeMutableRawPointer?, Double) -> Void
public typealias ChainSpeechFinish = @convention(c) (UnsafeMutableRawPointer?, Int32, UnsafePointer<CChar>?) -> Void

// Finish status codes — keep in sync with speech.rs's `analyzer` module.
private let statusOk: Int32 = 0
private let statusUnsupported: Int32 = 1
private let statusCancelled: Int32 = 2
private let statusFailed: Int32 = 3

private func finish(_ callback: ChainSpeechFinish, _ context: UnsafeMutableRawPointer?, _ status: Int32, _ payload: String) {
    payload.withCString { callback(context, status, $0) }
}

private func json(_ value: Any) -> String {
    guard let data = try? JSONSerialization.data(withJSONObject: value) else { return "null" }
    return String(decoding: data, as: UTF8.self)
}

private let lock = NSLock()
nonisolated(unsafe) private var running: Task<Void, Never>?

@_cdecl("chain_speech_analyzer_available")
public func chainSpeechAnalyzerAvailable() -> Bool {
    #if compiler(>=6.2)
    if #available(macOS 26, *) {
        return SpeechTranscriber.isAvailable
    }
    #endif
    return false
}

@_cdecl("chain_speech_analyzer_cancel")
public func chainSpeechAnalyzerCancel() {
    lock.lock()
    running?.cancel()
    lock.unlock()
}

@_cdecl("chain_speech_analyzer_locales")
public func chainSpeechAnalyzerLocales(_ context: UnsafeMutableRawPointer?, _ onFinish: ChainSpeechFinish) {
    #if compiler(>=6.2)
    if #available(macOS 26, *) {
        Task {
            let tags = await SpeechTranscriber.supportedLocales.map { $0.identifier(.bcp47) }
            finish(onFinish, context, statusOk, json(tags.sorted()))
        }
        return
    }
    #endif
    finish(onFinish, context, statusUnsupported, "SpeechAnalyzer needs macOS 26")
}

@_cdecl("chain_speech_analyzer_transcribe")
public func chainSpeechAnalyzerTranscribe(
    _ path: UnsafePointer<CChar>,
    _ localeTag: UnsafePointer<CChar>?,
    _ context: UnsafeMutableRawPointer?,
    _ onProgress: ChainSpeechProgress,
    _ onFinish: ChainSpeechFinish
) {
    #if compiler(>=6.2)
    if #available(macOS 26, *) {
        let url = URL(fileURLWithPath: String(cString: path))
        let requested = localeTag.map { String(cString: $0) }
        let task = Task {
            do {
                let (status, payload) = try await transcribe(url: url, requested: requested) { onProgress(context, $0) }
                finish(onFinish, context, status, payload)
            } catch is CancellationError {
                finish(onFinish, context, statusCancelled, "cancelled")
            } catch {
                finish(onFinish, context, Task.isCancelled ? statusCancelled : statusFailed, error.localizedDescription)
            }
        }
        lock.lock()
        running = task
        lock.unlock()
        return
    }
    #endif
    finish(onFinish, context, statusUnsupported, "SpeechAnalyzer needs macOS 26")
}

#if compiler(>=6.2)
@available(macOS 26, *)
private func transcribe(
    url: URL,
    requested: String?,
    progress: @escaping (Double) -> Void
) async throws -> (Int32, String) {
    let wanted = requested.map { Locale(identifier: $0) } ?? Locale.current
    guard let locale = await SpeechTranscriber.supportedLocale(equivalentTo: wanted) else {
        let name = requested ?? wanted.identifier(.bcp47)
        return (statusUnsupported, "no on-device speech model exists for \(name)")
    }
    let transcriber = SpeechTranscriber(
        locale: locale,
        transcriptionOptions: [],
        reportingOptions: [],
        attributeOptions: [.audioTimeRange]
    )
    // Downloads the locale's model the first time it's used.
    if let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
        try await request.downloadAndInstall()
    }

    let file = try AVAudioFile(forReading: url)
    let duration = Double(file.length) / file.processingFormat.sampleRate
    let analyzer = SpeechAnalyzer(modules: [transcriber])

    let collector = Task { () -> (words: [[String: Any]], text: [String]) in
        var words: [[String: Any]] = []
        var text: [String] = []
        for try await result in transcriber.results {
            text.append(String(result.text.characters).trimmingCharacters(in: .whitespaces))
            for run in result.text.runs {
                guard let range = run.audioTimeRange else { continue }
                let token = String(result.text[run.range].characters).trimmingCharacters(in: .whitespaces)
                if token.isEmpty { continue }
                words.append(["text": token, "start": range.start.seconds, "end": range.end.seconds])
                if duration > 0 { progress(min(range.end.seconds / duration, 0.99)) }
            }
        }
        return (words, text)
    }

    try await withTaskCancellationHandler {
        if let last = try await analyzer.analyzeSequence(from: file) {
            try await analyzer.finalizeAndFinish(through: last)
        } else {
            await analyzer.cancelAndFinishNow()
        }
    } onCancel: {
        collector.cancel()
        Task { await analyzer.cancelAndFinishNow() }
    }
    let collected = try await collector.value
    try Task.checkCancellation()
    progress(1)
    let text = collected.text.filter { !$0.isEmpty }.joined(separator: " ")
    return (statusOk, json(["words": collected.words, "text": text, "locale": locale.identifier(.bcp47)]))
}
#endif
