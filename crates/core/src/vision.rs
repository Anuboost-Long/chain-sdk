//! Vision capability — see /agent-docs/capabilities/vision/CONTRACT.md.
//! macOS reaches Vision's `VNRecognizeTextRequest` through objc2's
//! bindings; Windows reaches WinRT's `Windows.Media.Ocr` through the
//! `windows` crate. Linux is `Unsupported` for now.

use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum VisionError {
    /// The bytes aren't an image the OS can decode.
    InvalidImage(String),
    /// A requested language isn't available, or this platform has no recognizer.
    Unsupported(String),
    Other(String),
}

pub struct RecognizeOptions {
    pub languages: Vec<String>,
    pub accurate: bool,
}

#[derive(Debug, Serialize)]
pub struct TextBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Serialize)]
pub struct RecognizedLine {
    pub text: String,
    pub confidence: f64,
    #[serde(rename = "box")]
    pub bounds: TextBox,
}

#[derive(Debug, Serialize)]
pub struct RecognizedText {
    pub text: String,
    pub lines: Vec<RecognizedLine>,
}

impl RecognizedText {
    fn from_lines(lines: Vec<RecognizedLine>) -> Self {
        let text = lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n");
        Self { text, lines }
    }
}

#[derive(Debug, Serialize)]
pub struct DocumentParagraph {
    pub text: String,
    #[serde(rename = "box")]
    pub bounds: TextBox,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCell {
    pub text: String,
    #[serde(rename = "box")]
    pub bounds: TextBox,
    #[serde(skip_serializing_if = "is_one")]
    pub row_span: usize,
    #[serde(skip_serializing_if = "is_one")]
    pub col_span: usize,
}

fn is_one(span: &usize) -> bool {
    *span == 1
}

#[derive(Debug, Serialize)]
pub struct RecognizedTable {
    #[serde(rename = "box")]
    pub bounds: TextBox,
    pub rows: Vec<Vec<TableCell>>,
}

#[derive(Debug, Serialize)]
pub struct RecognizedList {
    pub items: Vec<String>,
    #[serde(rename = "box")]
    pub bounds: TextBox,
}

#[derive(Debug, Default, Serialize)]
pub struct RecognizedDocument {
    pub paragraphs: Vec<DocumentParagraph>,
    pub tables: Vec<RecognizedTable>,
    pub lists: Vec<RecognizedList>,
}

/// What swift/ChainVision.swift reports: boxes as `[x, y, width, height]`
/// with Vision's bottom-left origin, and every table cell with its row and
/// column range.
#[derive(Deserialize)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct NativeDocument {
    paragraphs: Vec<NativeText>,
    lists: Vec<NativeList>,
    tables: Vec<NativeTable>,
}

#[derive(Deserialize)]
struct NativeText {
    text: String,
    #[serde(rename = "box")]
    bounds: [f64; 4],
}

#[derive(Deserialize)]
struct NativeList {
    items: Vec<String>,
    #[serde(rename = "box")]
    bounds: [f64; 4],
}

#[derive(Deserialize)]
struct NativeTable {
    #[serde(rename = "box")]
    bounds: [f64; 4],
    cells: Vec<NativeCell>,
}

#[derive(Deserialize)]
struct NativeCell {
    text: String,
    #[serde(rename = "box")]
    bounds: [f64; 4],
    rows: [usize; 2],
    columns: [usize; 2],
    #[serde(default)]
    words: Vec<NativeText>,
}

fn flipped([x, y, width, height]: [f64; 4]) -> TextBox {
    TextBox { x, y: 1.0 - y - height, width, height }
}

fn contains_center(outer: &[f64; 4], inner: &[f64; 4]) -> bool {
    let (cx, cy) = (inner[0] + inner[2] / 2.0, inner[1] + inner[3] / 2.0);
    cx >= outer[0] && cx <= outer[0] + outer[2] && cy >= outer[1] && cy <= outer[1] + outer[3]
}

fn distance_to(outer: &[f64; 4], inner: &[f64; 4]) -> f64 {
    let (cx, cy) = (inner[0] + inner[2] / 2.0, inner[1] + inner[3] / 2.0);
    let dx = (outer[0] - cx).max(cx - (outer[0] + outer[2])).max(0.0);
    let dy = (outer[1] - cy).max(cy - (outer[1] + outer[3])).max(0.0);
    dx.hypot(dy)
}

/// With tight columns Vision gets the grid right but reads a line across
/// a cell border as one ("Requirements Quiz 1" in Lecture's cell, Lab's
/// empty). The words keep true boxes, so when any word lies outside its
/// own cell, the table's text is rebuilt by putting each word in the
/// cell it sits in.
fn reassign_misplaced_words(cells: &mut [NativeCell]) {
    let misplaced = cells.iter().any(|c| c.words.iter().any(|w| !contains_center(&c.bounds, &w.bounds)));
    if !misplaced {
        return;
    }
    let words: Vec<NativeText> = cells.iter_mut().flat_map(|c| std::mem::take(&mut c.words)).collect();
    let mut previous: Vec<Option<[f64; 4]>> = vec![None; cells.len()];
    for cell in cells.iter_mut() {
        cell.text.clear();
    }
    for word in words {
        let nearest = (0..cells.len())
            .min_by(|&a, &b| distance_to(&cells[a].bounds, &word.bounds).total_cmp(&distance_to(&cells[b].bounds, &word.bounds)))
            .expect("a table with words has cells");
        let cell = &mut cells[nearest];
        if let Some(last) = previous[nearest] {
            let below_last_line = word.bounds[1] + word.bounds[3] / 2.0 < last[1];
            cell.text.push(if below_last_line { '\n' } else { ' ' });
        }
        cell.text.push_str(&word.text);
        previous[nearest] = Some(word.bounds);
    }
}

/// Vision lists table cells and list items among its paragraphs too, so
/// paragraphs inside a table or list are dropped. A merged cell becomes one
/// cell in the row and column it starts at, with its spans — HTML's model.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn shape(native: Vec<NativeDocument>) -> RecognizedDocument {
    let mut document = RecognizedDocument::default();
    for page in native {
        let blocks: Vec<&[f64; 4]> =
            page.tables.iter().map(|t| &t.bounds).chain(page.lists.iter().map(|l| &l.bounds)).collect();
        document.paragraphs.extend(
            page.paragraphs
                .into_iter()
                .filter(|p| !blocks.iter().any(|block| contains_center(block, &p.bounds)))
                .map(|p| DocumentParagraph { text: p.text, bounds: flipped(p.bounds) }),
        );
        document.lists.extend(page.lists.into_iter().map(|l| RecognizedList { items: l.items, bounds: flipped(l.bounds) }));
        for table in page.tables {
            let mut cells = table.cells;
            cells.sort_by_key(|c| (c.rows[0], c.columns[0]));
            cells.dedup_by_key(|c| (c.rows, c.columns));
            reassign_misplaced_words(&mut cells);
            let row_count = cells.iter().map(|c| c.rows[1] + 1).max().unwrap_or(0);
            let mut rows: Vec<Vec<TableCell>> = (0..row_count).map(|_| Vec::new()).collect();
            for cell in cells {
                rows[cell.rows[0]].push(TableCell {
                    text: cell.text,
                    bounds: flipped(cell.bounds),
                    row_span: cell.rows[1] - cell.rows[0] + 1,
                    col_span: cell.columns[1] - cell.columns[0] + 1,
                });
            }
            document.tables.push(RecognizedTable { bounds: flipped(table.bounds), rows });
        }
    }
    document
}

/// Maps each requested BCP-47 tag onto one the recognizer supports: an
/// exact match (case-insensitive), else a bare language (`fr`) onto its
/// first supported variant (`fr-FR`). The first tag with neither is the error.
fn resolve_languages(requested: &[String], supported: &[String]) -> Result<Vec<String>, VisionError> {
    requested
        .iter()
        .map(|tag| {
            let exact = supported.iter().find(|s| s.eq_ignore_ascii_case(tag));
            let by_language = || {
                supported.iter().find(|s| {
                    s.split('-').next().is_some_and(|lang| lang.eq_ignore_ascii_case(tag))
                })
            };
            exact
                .or_else(by_language)
                .cloned()
                .ok_or_else(|| VisionError::Unsupported(format!("text recognition doesn't support the language {tag:?}")))
        })
        .collect()
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::rc::{autoreleasepool, Retained};
    use objc2::AllocAnyThread;
    use objc2_core_foundation::CFData;
    use objc2_foundation::{NSArray, NSDictionary, NSString};
    use objc2_image_io::CGImageSource;
    use objc2_vision::{
        VNImageOption, VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
    };

    use super::*;

    fn text_request(accurate: bool) -> Retained<VNRecognizeTextRequest> {
        let request = VNRecognizeTextRequest::new();
        request.setRecognitionLevel(if accurate {
            VNRequestTextRecognitionLevel::Accurate
        } else {
            VNRequestTextRecognitionLevel::Fast
        });
        request.setUsesLanguageCorrection(accurate);
        request
    }

    fn supported(request: &VNRecognizeTextRequest) -> Result<Vec<String>, VisionError> {
        let languages = unsafe { request.supportedRecognitionLanguagesAndReturnError() }
            .map_err(|e| VisionError::Other(e.localizedDescription().to_string()))?;
        Ok(languages.iter().map(|tag| tag.to_string()).collect())
    }

    /// ImageIO decodes everything Vision can read; checking here first is
    /// what separates "not an image" from a recognizer failure.
    fn is_decodable(image: &[u8]) -> bool {
        let data = CFData::from_bytes(image);
        let Some(source) = (unsafe { CGImageSource::with_data(&data, None) }) else {
            return false;
        };
        unsafe { source.count() > 0 && source.image_at_index(0, None).is_some() }
    }

    pub fn languages() -> Result<Vec<String>, VisionError> {
        autoreleasepool(|_| supported(&text_request(true)))
    }

    pub fn recognize_text(image: &[u8], options: &RecognizeOptions) -> Result<RecognizedText, VisionError> {
        if image.is_empty() || !is_decodable(image) {
            return Err(VisionError::InvalidImage("the bytes aren't an image the OS can decode".to_string()));
        }
        autoreleasepool(|_| {
            let request = text_request(options.accurate);
            if options.languages.is_empty() {
                request.setAutomaticallyDetectsLanguage(true);
            } else {
                let languages = resolve_languages(&options.languages, &supported(&request)?)?;
                let languages: Vec<Retained<NSString>> = languages.iter().map(|l| NSString::from_str(l)).collect();
                request.setRecognitionLanguages(&NSArray::from_retained_slice(&languages));
            }

            // initWithData honors a photo's EXIF orientation.
            let data = objc2_foundation::NSData::with_bytes(image);
            let handler = VNImageRequestHandler::initWithData_options(
                VNImageRequestHandler::alloc(),
                &data,
                &NSDictionary::<VNImageOption, objc2::runtime::AnyObject>::new(),
            );
            let requests: Retained<NSArray<VNRequest>> =
                NSArray::from_slice(&[&**request]);
            handler
                .performRequests_error(&requests)
                .map_err(|e| VisionError::Other(e.localizedDescription().to_string()))?;

            let lines = request
                .results()
                .map(|observations| {
                    observations
                        .iter()
                        .filter_map(|observation| {
                            let candidate = observation.topCandidates(1).firstObject()?;
                            // Vision's box is normalized with a bottom-left origin.
                            let rect = unsafe { observation.boundingBox() };
                            Some(RecognizedLine {
                                text: candidate.string().to_string(),
                                confidence: f64::from(candidate.confidence()),
                                bounds: TextBox {
                                    x: rect.origin.x,
                                    y: 1.0 - rect.origin.y - rect.size.height,
                                    width: rect.size.width,
                                    height: rect.size.height,
                                },
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(RecognizedText::from_lines(lines))
        })
    }
}

#[cfg(target_os = "macos")]
mod document {
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::sync::mpsc;

    use super::*;

    const STATUS_OK: i32 = 0;
    const STATUS_UNSUPPORTED: i32 = 1;
    const STATUS_INVALID_IMAGE: i32 = 2;

    type Finish = extern "C" fn(*mut c_void, i32, *const c_char);

    extern "C" {
        fn chain_vision_recognize_document(
            bytes: *const u8,
            length: usize,
            languages_json: *const c_char,
            context: *mut c_void,
            on_finish: Finish,
        );
    }

    /// `context` is a leaked `Sender`; Swift calls back exactly once.
    extern "C" fn forward_finish(context: *mut c_void, status: i32, payload: *const c_char) {
        let sender = unsafe { &*(context as *const mpsc::Sender<(i32, String)>) };
        let payload = unsafe { CStr::from_ptr(payload) }.to_string_lossy().into_owned();
        let _ = sender.send((status, payload));
    }

    pub fn recognize_document(image: &[u8], languages: &[String]) -> Result<RecognizedDocument, VisionError> {
        if image.is_empty() {
            return Err(VisionError::InvalidImage("the bytes aren't an image the OS can decode".to_string()));
        }
        let languages = if languages.is_empty() { Vec::new() } else { resolve_languages(languages, &super::macos::languages()?)? };
        let languages = CString::new(serde_json::to_string(&languages).expect("strings serialize")).expect("JSON has no NUL");

        let (tx, rx) = mpsc::channel::<(i32, String)>();
        let context = Box::into_raw(Box::new(tx));
        unsafe { chain_vision_recognize_document(image.as_ptr(), image.len(), languages.as_ptr(), context.cast(), forward_finish) };
        let (status, payload) = rx.recv().expect("the context keeps a sender alive");
        drop(unsafe { Box::from_raw(context) });

        match status {
            STATUS_OK => serde_json::from_str::<Vec<NativeDocument>>(&payload)
                .map(shape)
                .map_err(|e| VisionError::Other(format!("unexpected document result: {e}"))),
            STATUS_UNSUPPORTED => Err(VisionError::Unsupported(payload)),
            STATUS_INVALID_IMAGE => Err(VisionError::InvalidImage(payload)),
            _ => Err(VisionError::Other(payload)),
        }
    }
}

#[cfg(target_os = "macos")]
pub use document::recognize_document;
#[cfg(target_os = "macos")]
pub use macos::{languages, recognize_text};

#[cfg(not(target_os = "macos"))]
pub fn recognize_document(_image: &[u8], _languages: &[String]) -> Result<RecognizedDocument, VisionError> {
    Err(VisionError::Unsupported("document recognition isn't available on this platform yet".to_string()))
}

#[cfg(windows)]
mod windows_ocr {
    use windows::core::HSTRING;
    use windows::Globalization::Language;
    use windows::Graphics::Imaging::{BitmapDecoder, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

    use super::*;

    fn native(e: windows::core::Error) -> VisionError {
        VisionError::Other(e.message().to_string())
    }

    /// WinRT needs the calling thread in an apartment; the command runs on
    /// a pooled blocking thread that may not be. "Already initialized" is fine.
    fn init_apartment() {
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    }

    fn supported() -> Result<Vec<String>, VisionError> {
        OcrEngine::AvailableRecognizerLanguages()
            .map_err(native)?
            .into_iter()
            .map(|language| language.LanguageTag().map(|tag| tag.to_string()).map_err(native))
            .collect()
    }

    fn decode(image: &[u8]) -> Result<SoftwareBitmap, VisionError> {
        let invalid = |_| VisionError::InvalidImage("the bytes aren't an image the OS can decode".to_string());
        let stream = InMemoryRandomAccessStream::new().map_err(native)?;
        let writer = DataWriter::CreateDataWriter(&stream).map_err(native)?;
        writer.WriteBytes(image).map_err(native)?;
        writer.StoreAsync().map_err(native)?.get().map_err(native)?;
        writer.DetachStream().map_err(native)?;
        stream.Seek(0).map_err(native)?;
        let decoder = BitmapDecoder::CreateAsync(&stream).map_err(native)?.get().map_err(invalid)?;
        decoder.GetSoftwareBitmapAsync().map_err(native)?.get().map_err(invalid)
    }

    pub fn languages() -> Result<Vec<String>, VisionError> {
        init_apartment();
        supported()
    }

    /// `accurate` has no Windows equivalent — the one engine is used either way.
    pub fn recognize_text(image: &[u8], options: &RecognizeOptions) -> Result<RecognizedText, VisionError> {
        if image.is_empty() {
            return Err(VisionError::InvalidImage("the bytes aren't an image the OS can decode".to_string()));
        }
        init_apartment();
        // OcrEngine takes one language: every requested one must exist,
        // and the first is used.
        let engine = match resolve_languages(&options.languages, &supported()?)?.first() {
            Some(tag) => {
                OcrEngine::TryCreateFromLanguage(&Language::CreateLanguage(&HSTRING::from(tag)).map_err(native)?)
            }
            None => OcrEngine::TryCreateFromUserProfileLanguages(),
        }
        .map_err(|_| VisionError::Unsupported("no OCR language pack is installed".to_string()))?;

        let bitmap = decode(image)?;
        let max = OcrEngine::MaxImageDimension().map_err(native)?;
        let (width, height) = (bitmap.PixelWidth().map_err(native)?, bitmap.PixelHeight().map_err(native)?);
        if width as u32 > max || height as u32 > max {
            return Err(VisionError::InvalidImage(format!(
                "the image is {width}×{height}; Windows OCR reads images up to {max} pixels on a side"
            )));
        }
        let result = engine.RecognizeAsync(&bitmap).map_err(native)?.get().map_err(native)?;

        let (width, height) = (f64::from(width), f64::from(height));
        let mut lines = Vec::new();
        for line in result.Lines().map_err(native)? {
            // A line's box is the union of its words' pixel boxes.
            let (mut left, mut top, mut right, mut bottom) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for word in line.Words().map_err(native)? {
                let rect = word.BoundingRect().map_err(native)?;
                left = left.min(f64::from(rect.X));
                top = top.min(f64::from(rect.Y));
                right = right.max(f64::from(rect.X + rect.Width));
                bottom = bottom.max(f64::from(rect.Y + rect.Height));
            }
            if left > right {
                continue;
            }
            lines.push(RecognizedLine {
                text: line.Text().map_err(native)?.to_string(),
                // Windows OCR reports no confidence.
                confidence: 1.0,
                bounds: TextBox {
                    x: left / width,
                    y: top / height,
                    width: (right - left) / width,
                    height: (bottom - top) / height,
                },
            });
        }
        Ok(RecognizedText::from_lines(lines))
    }
}

#[cfg(windows)]
pub use windows_ocr::{languages, recognize_text};

#[cfg(not(any(target_os = "macos", windows)))]
pub fn languages() -> Result<Vec<String>, VisionError> {
    Err(VisionError::Unsupported("text recognition isn't available on this platform yet".to_string()))
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn recognize_text(_image: &[u8], _options: &RecognizeOptions) -> Result<RecognizedText, VisionError> {
    Err(VisionError::Unsupported("text recognition isn't available on this platform yet".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_exact_and_bare_language_tags() {
        let supported = vec!["en-US".to_string(), "fr-FR".to_string(), "zh-Hans".to_string()];
        let requested = vec!["EN-us".to_string(), "fr".to_string(), "zh-Hans".to_string()];
        assert_eq!(resolve_languages(&requested, &supported).unwrap(), vec!["en-US", "fr-FR", "zh-Hans"]);
        assert!(matches!(
            resolve_languages(&["tlh".to_string()], &supported),
            Err(VisionError::Unsupported(m)) if m.contains("tlh")
        ));
    }

    #[test]
    fn text_joins_lines_with_newlines() {
        let line = |text: &str| RecognizedLine {
            text: text.to_string(),
            confidence: 1.0,
            bounds: TextBox { x: 0.0, y: 0.0, width: 1.0, height: 0.1 },
        };
        assert_eq!(RecognizedText::from_lines(vec![line("a"), line("b")]).text, "a\nb");
        assert_eq!(RecognizedText::from_lines(vec![]).text, "");
    }

    #[test]
    fn shapes_tables_with_spans_and_drops_paragraphs_inside_blocks() {
        let native: Vec<NativeDocument> = serde_json::from_str(
            r#"[{
              "paragraphs": [
                {"text": "Title", "box": [0.1, 0.9, 0.2, 0.05]},
                {"text": "Sales", "box": [0.4, 0.7, 0.06, 0.03]},
                {"text": "• item", "box": [0.1, 0.3, 0.2, 0.03]}
              ],
              "lists": [{"items": ["item"], "box": [0.05, 0.25, 0.3, 0.1]}],
              "tables": [{
                "box": [0.05, 0.5, 0.75, 0.25],
                "cells": [
                  {"text": "Sales", "box": [0.3, 0.68, 0.36, 0.06], "rows": [0, 0], "columns": [1, 2]},
                  {"text": "Region", "box": [0.05, 0.68, 0.2, 0.06], "rows": [0, 0], "columns": [0, 0]},
                  {"text": "Region", "box": [0.05, 0.68, 0.2, 0.06], "rows": [0, 0], "columns": [0, 0]},
                  {"text": "EU", "box": [0.05, 0.6, 0.2, 0.06], "rows": [1, 1], "columns": [0, 0]},
                  {"text": "120", "box": [0.3, 0.6, 0.18, 0.06], "rows": [1, 1], "columns": [1, 1]},
                  {"text": "140", "box": [0.48, 0.6, 0.18, 0.06], "rows": [1, 1], "columns": [2, 2]}
                ]
              }]
            }]"#,
        )
        .unwrap();
        let document = shape(native);

        assert_eq!(document.paragraphs.iter().map(|p| p.text.as_str()).collect::<Vec<_>>(), ["Title"]);
        assert!((document.paragraphs[0].bounds.y - 0.05).abs() < 1e-9);
        let table = &document.tables[0];
        let texts = |r: usize| table.rows[r].iter().map(|c| c.text.as_str()).collect::<Vec<_>>();
        assert_eq!(texts(0), ["Region", "Sales"]);
        assert_eq!(texts(1), ["EU", "120", "140"]);
        assert_eq!((table.rows[0][1].row_span, table.rows[0][1].col_span), (1, 2));
        let json = serde_json::to_value(&table.rows[0]).unwrap();
        assert_eq!(json[0].get("colSpan"), None);
        assert_eq!(json[1]["colSpan"], 2);
        assert_eq!(document.lists[0].items, ["item"]);
    }

    #[test]
    fn moves_words_read_across_a_tight_column_border() {
        let word = |text: &str, x: f64, width: f64, y: f64| NativeText { text: text.to_string(), bounds: [x, y, width, 0.02] };
        let cell = |column: usize, x: f64, width: f64, text: &str, words: Vec<NativeText>| NativeCell {
            text: text.to_string(),
            bounds: [x, 0.78, width, 0.05],
            rows: [0, 0],
            columns: [column, column],
            words,
        };
        let mut cells = vec![
            cell(0, 0.14, 0.11, "Requirements Quiz 1", vec![
                word("Requirements", 0.137, 0.116, 0.8),
                word("Quiz", 0.256, 0.038, 0.8),
                word("1", 0.296, 0.014, 0.8),
            ]),
            cell(1, 0.256, 0.056, "", vec![]),
        ];
        reassign_misplaced_words(&mut cells);
        assert_eq!((cells[0].text.as_str(), cells[1].text.as_str()), ("Requirements", "Quiz 1"));

        let mut two_lines = vec![cell(0, 0.0, 0.5, "keep me", vec![word("a", 0.1, 0.05, 0.81), word("b", 0.1, 0.05, 0.785)])];
        reassign_misplaced_words(&mut two_lines);
        assert_eq!(two_lines[0].text, "keep me", "untouched when every word is inside its cell");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recognizes_a_table_with_a_merged_header() {
        let image = include_bytes!("../tests/fixtures/document-with-table.png");
        let document = match recognize_document(image, &[]) {
            Err(VisionError::Unsupported(_)) => return, // before macOS 26
            result => result.unwrap(),
        };
        let texts = |r: &[TableCell]| r.iter().map(|c| c.text.clone()).collect::<Vec<_>>();
        let table = &document.tables[0];
        assert_eq!(texts(&table.rows[0]), ["Region", "Sales", "Growth"]);
        assert_eq!(table.rows[0][1].col_span, 2);
        assert_eq!(texts(&table.rows[3]), ["Americas", "200", "204", "2%"]);
        assert_eq!(document.lists[0].items, ["Europe led growth", "Asia recovered", "Americas were flat"]);
        let paragraphs: Vec<_> = document.paragraphs.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(paragraphs.first(), Some(&"Quarterly Report"));
        assert!(paragraphs.contains(&"Key points:"));
        assert!(!paragraphs.contains(&"Sales") && !paragraphs.iter().any(|p| p.contains("Asia recovered")));
        assert!(document.paragraphs[0].bounds.y < 0.1, "top-left origin");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn keeps_cells_apart_when_columns_are_tight() {
        let image = include_bytes!("../tests/fixtures/table-tight-columns.png");
        let document = match recognize_document(image, &[]) {
            Err(VisionError::Unsupported(_)) => return,
            result => result.unwrap(),
        };
        let rows: Vec<Vec<String>> =
            document.tables[0].rows.iter().map(|r| r.iter().map(|c| c.text.clone()).collect()).collect();
        assert_eq!(rows[1], ["Monday", "Requirements", "Quiz 1"]);
        assert_eq!(rows[2], ["Wednesday", "Design", "Lab 2"]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn rejects_bytes_that_are_not_an_image() {
        let options = RecognizeOptions { languages: vec![], accurate: true };
        assert!(matches!(recognize_text(b"not an image", &options), Err(VisionError::InvalidImage(_))));
        assert!(matches!(recognize_text(&[], &options), Err(VisionError::InvalidImage(_))));
        assert!(matches!(recognize_document(b"not an image", &[]), Err(VisionError::InvalidImage(_))));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn lists_languages_including_english() {
        assert!(languages().unwrap().iter().any(|l| l.starts_with("en")));
    }
}
