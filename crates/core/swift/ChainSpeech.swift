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
/// Fills up to `capacity` mono samples; returns how many, 0 at the end, -1 on a read error.
public typealias ChainSpeechRead = @convention(c) (UnsafeMutableRawPointer?, UnsafeMutablePointer<Float>, Int) -> Int

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
        run(context, onFinish) {
            let file = try AVAudioFile(forReading: url)
            let duration = Double(file.length) / file.processingFormat.sampleRate
            return try await transcribe(requested: requested, duration: duration, progress: { onProgress(context, $0) }) { analyzer, _ in
                try await analyzer.analyzeSequence(from: file)
            }
        }
        return
    }
    #endif
    finish(onFinish, context, statusUnsupported, "SpeechAnalyzer needs macOS 26")
}

/// Like `chain_speech_analyzer_transcribe`, but the audio is mono samples
/// at `sampleRate` pulled through `read` — sound Chain decodes itself
/// because AVFoundation can't (WebM, Opus). Progress is the reader's to
/// report, so `onProgress` only gets the final 1.
@_cdecl("chain_speech_analyzer_transcribe_samples")
public func chainSpeechAnalyzerTranscribeSamples(
    _ sampleRate: Double,
    _ localeTag: UnsafePointer<CChar>?,
    _ reader: UnsafeMutableRawPointer?,
    _ read: ChainSpeechRead,
    _ context: UnsafeMutableRawPointer?,
    _ onProgress: ChainSpeechProgress,
    _ onFinish: ChainSpeechFinish
) {
    #if compiler(>=6.2)
    if #available(macOS 26, *) {
        let requested = localeTag.map { String(cString: $0) }
        run(context, onFinish) {
            let samples = SampleSource(read: read, reader: reader, sampleRate: sampleRate)
            // Rust frees the reader once we finish, so no read may outlive this.
            defer { samples.close() }
            return try await transcribe(requested: requested, duration: 0, progress: { onProgress(context, $0) }) { analyzer, transcriber in
                let target = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber])
                return try await analyzer.analyzeSequence(samples.stream(to: target))
            }
        }
        return
    }
    #endif
    finish(onFinish, context, statusUnsupported, "SpeechAnalyzer needs macOS 26")
}

/// Runs one transcription as the cancellable `running` task.
private func run(
    _ context: UnsafeMutableRawPointer?,
    _ onFinish: ChainSpeechFinish,
    _ body: @escaping () async throws -> (Int32, String)
) {
    let task = Task {
        do {
            let (status, payload) = try await body()
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
}

#if compiler(>=6.2)
/// Pulls mono Float32 samples from Rust in chunks and converts them to the
/// analyzer's format. Pulled one chunk at a time, so a long file never
/// sits in memory.
@available(macOS 26, *)
private final class SampleSource: @unchecked Sendable {
    private static let chunk: AVAudioFrameCount = 8192
    private let read: ChainSpeechRead
    private let reader: UnsafeMutableRawPointer?
    private let source: AVAudioFormat
    private var target: AVAudioFormat
    private var converter: AVAudioConverter?
    private var ended = false
    private let lock = NSLock()

    init(read: @escaping ChainSpeechRead, reader: UnsafeMutableRawPointer?, sampleRate: Double) {
        self.read = read
        self.reader = reader
        source = AVAudioFormat(standardFormatWithSampleRate: sampleRate, channels: 1)!
        target = source
    }

    /// The samples converted to `format` (nil: as they are).
    func stream(to format: AVAudioFormat?) -> AsyncStream<AnalyzerInput> {
        if let format, format != source {
            target = format
            converter = AVAudioConverter(from: source, to: format)
        }
        return AsyncStream(unfolding: { self.next() })
    }

    /// Ends the stream, waiting out a read in progress.
    func close() {
        lock.lock()
        ended = true
        lock.unlock()
    }

    private func next() -> AnalyzerInput? {
        lock.lock()
        defer { lock.unlock() }
        while !ended {
            guard let input = AVAudioPCMBuffer(pcmFormat: source, frameCapacity: Self.chunk) else { return nil }
            let count = read(reader, input.floatChannelData![0], Int(Self.chunk))
            input.frameLength = AVAudioFrameCount(max(count, 0))
            ended = count <= 0
            guard let output = convert(input) else { return nil }
            if output.frameLength > 0 { return AnalyzerInput(buffer: output) }
        }
        return nil
    }

    /// An empty `input` flushes the converter's tail.
    private func convert(_ input: AVAudioPCMBuffer) -> AVAudioPCMBuffer? {
        guard let converter else { return input }
        let capacity = AVAudioFrameCount(Double(Self.chunk) * target.sampleRate / source.sampleRate) + 1024
        guard let output = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else { return nil }
        var supplied = false
        converter.convert(to: output, error: nil) { _, status in
            if input.frameLength == 0 {
                status.pointee = .endOfStream
                return nil
            }
            if supplied {
                status.pointee = .noDataNow
                return nil
            }
            supplied = true
            status.pointee = .haveData
            return input
        }
        return output
    }
}

/// Sets up the locale's transcriber, feeds it through `analyze`, and
/// collects words and text. `duration` (seconds, 0 when unknown) turns
/// recognized-word times into progress.
@available(macOS 26, *)
private func transcribe(
    requested: String?,
    duration: Double,
    progress: @escaping (Double) -> Void,
    analyze: @escaping (SpeechAnalyzer, SpeechTranscriber) async throws -> CMTime?
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
        if let last = try await analyze(analyzer, transcriber) {
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
