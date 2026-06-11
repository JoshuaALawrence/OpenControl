use serde_json::{json, Value};

use crate::capture::Bitmap;

/// Recognize text in an already-captured bitmap. `origin_x/origin_y` of the
/// bitmap map word boxes back to absolute screen coordinates.
pub fn recognize_bitmap(bm: Bitmap) -> Result<Value, String> {
    // Run all WinRT work on a dedicated MTA thread to avoid STA deadlock when
    // blocking on RecognizeAsync().get().
    let handle = std::thread::spawn(move || -> Result<Value, String> { recognize_inner(bm) });
    handle
        .join()
        .map_err(|_| "ocr thread panicked".to_string())?
}

fn recognize_inner(bm: Bitmap) -> Result<Value, String> {
    use windows::Foundation::Rect;
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Security::Cryptography::CryptographicBuffer;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    if bm.width <= 0 || bm.height <= 0 {
        return Err("empty image for OCR".into());
    }

    // RGBA (our capture format) -> BGRA for BitmapPixelFormat::Bgra8.
    let mut bgra = bm.rgba.clone();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }

    let buffer = CryptographicBuffer::CreateFromByteArray(&bgra)
        .map_err(|e| format!("CreateFromByteArray failed: {e}"))?;
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        bm.width,
        bm.height,
    )
    .map_err(|e| format!("CreateCopyFromBuffer failed: {e}"))?;

    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|e| format!("OCR engine unavailable: {e}"))?;

    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(|e| format!("RecognizeAsync failed: {e}"))?
        .get()
        .map_err(|e| format!("OCR await failed: {e}"))?;

    let full_text = result.Text().map(|s| s.to_string()).unwrap_or_default();

    let mut words_out: Vec<Value> = Vec::new();
    let mut lines_out: Vec<String> = Vec::new();
    if let Ok(lines) = result.Lines() {
        let n = lines.Size().unwrap_or(0);
        for i in 0..n {
            let line = match lines.GetAt(i) {
                Ok(l) => l,
                Err(_) => continue,
            };
            if let Ok(t) = line.Text() {
                lines_out.push(t.to_string());
            }
            if let Ok(words) = line.Words() {
                let wn = words.Size().unwrap_or(0);
                for j in 0..wn {
                    let word = match words.GetAt(j) {
                        Ok(w) => w,
                        Err(_) => continue,
                    };
                    let text = word.Text().map(|s| s.to_string()).unwrap_or_default();
                    let rect: Rect = word.BoundingRect().unwrap_or(Rect {
                        X: 0.0,
                        Y: 0.0,
                        Width: 0.0,
                        Height: 0.0,
                    });
                    let sx = bm.origin_x + rect.X.round() as i32;
                    let sy = bm.origin_y + rect.Y.round() as i32;
                    let w = rect.Width.round() as i32;
                    let h = rect.Height.round() as i32;
                    words_out.push(json!({
                        "text": text,
                        "x": sx, "y": sy, "width": w, "height": h,
                        "center": { "x": sx + w / 2, "y": sy + h / 2 }
                    }));
                }
            }
        }
    }

    Ok(json!({
        "text": full_text,
        "lines": lines_out,
        "words": words_out,
        "origin": { "x": bm.origin_x, "y": bm.origin_y },
    }))
}

/// List installed OCR recognizer languages (BCP-47 tags), best effort.
pub fn available_languages() -> Vec<String> {
    let handle = std::thread::spawn(|| -> Vec<String> {
        use windows::Media::Ocr::OcrEngine;
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let mut out = Vec::new();
        if let Ok(langs) = OcrEngine::AvailableRecognizerLanguages() {
            let n = langs.Size().unwrap_or(0);
            for i in 0..n {
                if let Ok(lang) = langs.GetAt(i) {
                    if let Ok(tag) = lang.LanguageTag() {
                        out.push(tag.to_string());
                    }
                }
            }
        }
        out
    });
    handle.join().unwrap_or_default()
}
