// RecognizeDocumentsRequest bridge for crates/core/src/vision.rs — see
// agent-docs/capabilities/vision/research/MACOS.md. The request is
// Swift-only (macOS 26), so like ChainSpeech.swift this exposes a plain C
// function and hands back JSON. Boxes stay in Vision's bottom-left
// coordinates; vision.rs flips them and shapes the result.

import CoreGraphics
import Foundation
import ImageIO
import Vision

public typealias ChainVisionFinish = @convention(c) (UnsafeMutableRawPointer?, Int32, UnsafePointer<CChar>?) -> Void

// Keep in sync with vision.rs's `document` module.
private let statusOk: Int32 = 0
private let statusUnsupported: Int32 = 1
private let statusInvalidImage: Int32 = 2
private let statusFailed: Int32 = 3

private func finish(_ callback: ChainVisionFinish, _ context: UnsafeMutableRawPointer?, _ status: Int32, _ payload: String) {
    payload.withCString { callback(context, status, $0) }
}

@_cdecl("chain_vision_recognize_document")
public func chainVisionRecognizeDocument(
    _ bytes: UnsafePointer<UInt8>,
    _ length: Int,
    _ languagesJSON: UnsafePointer<CChar>,
    _ context: UnsafeMutableRawPointer?,
    _ onFinish: ChainVisionFinish
) {
    #if compiler(>=6.2)
    if #available(macOS 26, *) {
        let data = Data(bytes: bytes, count: length)
        let languages = (try? JSONSerialization.jsonObject(with: Data(String(cString: languagesJSON).utf8))) as? [String] ?? []
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
        else {
            finish(onFinish, context, statusInvalidImage, "the bytes aren't an image the OS can decode")
            return
        }
        let orientation = (CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any])?[kCGImagePropertyOrientation]
            .flatMap { ($0 as? UInt32).flatMap(CGImagePropertyOrientation.init(rawValue:)) }
        Task {
            var request = RecognizeDocumentsRequest()
            request.textRecognitionOptions.automaticallyDetectLanguage = languages.isEmpty
            request.textRecognitionOptions.recognitionLanguages = languages.map { Locale.Language(identifier: $0) }
            do {
                let observations = try await request.perform(on: image, orientation: orientation)
                finish(onFinish, context, statusOk, json(observations.map(document)))
            } catch {
                finish(onFinish, context, statusFailed, error.localizedDescription)
            }
        }
        return
    }
    #endif
    finish(onFinish, context, statusUnsupported, "document recognition needs macOS 26 or later")
}

private func json(_ value: Any) -> String {
    guard let data = try? JSONSerialization.data(withJSONObject: value) else { return "[]" }
    return String(decoding: data, as: UTF8.self)
}

#if compiler(>=6.2)
@available(macOS 26, *)
private func box(_ region: NormalizedRegion) -> [Double] {
    box(region.boundingBox)
}

@available(macOS 26, *)
private func box(_ rect: NormalizedRect) -> [Double] {
    [rect.cgRect.origin.x, rect.cgRect.origin.y, rect.cgRect.width, rect.cgRect.height].map(Double.init)
}

@available(macOS 26, *)
private func document(_ observation: DocumentObservation) -> [String: Any] {
    let document = observation.document
    return [
        "paragraphs": document.paragraphs.map { ["text": $0.transcript, "box": box($0.boundingRegion)] },
        "lists": document.lists.map { ["items": $0.items.map(\.itemString), "box": box($0.boundingRegion)] },
        "tables": document.tables.map { table in
            [
                "box": box(table.boundingRegion),
                "cells": table.rows.flatMap { $0 }.map { cell in
                    [
                        "text": cell.content.text.transcript,
                        "box": box(cell.content.boundingRegion),
                        "rows": [cell.rowRange.lowerBound, cell.rowRange.upperBound],
                        "columns": [cell.columnRange.lowerBound, cell.columnRange.upperBound],
                        "words": (cell.content.text.words ?? []).map { word in
                            ["text": word.topCandidates(1).first?.string ?? "", "box": box(word.boundingBox)] as [String: Any]
                        },
                    ] as [String: Any]
                },
            ] as [String: Any]
        },
    ]
}
#endif
