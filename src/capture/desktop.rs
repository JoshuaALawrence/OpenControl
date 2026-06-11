use base64::Engine;
use image::{Rgba, RgbaImage};
use serde_json::{json, Value};
use std::io::Cursor;
use std::sync::Mutex;
use windows::core::w;
use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateDCW, DeleteDC, DeleteObject,
    EnumDisplayMonitors, GetDIBits, GetMonitorInfoW, SelectObject, BITMAPINFO, BITMAPINFOHEADER,
    DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, HMONITOR, MONITORINFO, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};

use crate::capture::Bitmap;

// ---------------------------------------------------------------------------
// View state: maps model image-space coordinates back to physical screen pixels.
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Debug)]
pub struct ViewState {
    pub offset_x: i32,
    pub offset_y: i32,
    pub scale: f64, // image_px = physical_px * scale
}

impl Default for ViewState {
    fn default() -> Self {
        ViewState {
            offset_x: 0,
            offset_y: 0,
            scale: 1.0,
        }
    }
}

static VIEW: Mutex<ViewState> = Mutex::new(ViewState {
    offset_x: 0,
    offset_y: 0,
    scale: 1.0,
});

pub fn set_view(offset_x: i32, offset_y: i32, scale: f64) {
    let mut v = VIEW.lock().unwrap();
    v.offset_x = offset_x;
    v.offset_y = offset_y;
    v.scale = if scale > 0.0 { scale } else { 1.0 };
}

/// Convert model image-space coords to physical virtual-desktop pixels.
pub fn to_screen(x: i32, y: i32) -> (i32, i32) {
    let v = *VIEW.lock().unwrap();
    let sx = v.offset_x as f64 + x as f64 / v.scale;
    let sy = v.offset_y as f64 + y as f64 / v.scale;
    (sx.round() as i32, sy.round() as i32)
}

// ---------------------------------------------------------------------------
// Monitor enumeration
// ---------------------------------------------------------------------------
pub fn virtual_bounds() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

extern "system" fn mon_enum(hmon: HMONITOR, _hdc: HDC, _rc: *mut RECT, lparam: LPARAM) -> BOOL {
    unsafe {
        let acc = &mut *(lparam.0 as *mut Vec<(i32, i32, i32, i32, bool)>);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            let r = mi.rcMonitor;
            let primary = (mi.dwFlags & 1) != 0; // MONITORINFOF_PRIMARY
            acc.push((r.left, r.top, r.right - r.left, r.bottom - r.top, primary));
        }
        BOOL(1)
    }
}

pub fn list_monitors() -> Value {
    let acc = monitor_rects();
    let (vx, vy, vw, vh) = virtual_bounds();
    let mons: Vec<Value> = acc
        .iter()
        .enumerate()
        .map(|(i, (x, y, w, h, primary))| {
            json!({ "index": i + 1, "left": x, "top": y, "width": w, "height": h, "primary": primary })
        })
        .collect();
    json!({
        "monitors": mons,
        "virtual_desktop": { "left": vx, "top": vy, "width": vw, "height": vh },
        "note": "index 1 = first monitor; use a monitor index or an absolute [x,y,w,h] region with take_screenshot."
    })
}

fn monitor_rects() -> Vec<(i32, i32, i32, i32, bool)> {
    let mut acc: Vec<(i32, i32, i32, i32, bool)> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(mon_enum),
            LPARAM(&mut acc as *mut _ as isize),
        );
    }
    acc
}

// ---------------------------------------------------------------------------
// GDI screen capture (whole desktop / monitor / region)
// ---------------------------------------------------------------------------
/// Capture an absolute virtual-desktop rectangle into an RGBA bitmap.
pub fn capture_region(x: i32, y: i32, width: i32, height: i32) -> Result<Bitmap, String> {
    if width <= 0 || height <= 0 {
        return Err("capture region has zero size".into());
    }
    unsafe {
        // A DC over the whole virtual display handles negative/multi-monitor coords.
        let screen_dc = CreateDCW(w!("DISPLAY"), None, None, None);
        if screen_dc.0.is_null() {
            return Err("CreateDCW(DISPLAY) failed".into());
        }
        let mem_dc = CreateCompatibleDC(screen_dc);
        let bmp: HBITMAP = CreateCompatibleBitmap(screen_dc, width, height);
        let old = SelectObject(mem_dc, HGDIOBJ(bmp.0));

        let ok = BitBlt(mem_dc, 0, 0, width, height, screen_dc, x, y, SRCCOPY).is_ok();
        let result = if ok {
            read_dibits(mem_dc, bmp, width, height)
        } else {
            Err("BitBlt failed".into())
        };

        SelectObject(mem_dc, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem_dc);
        let _ = DeleteDC(screen_dc);

        result.map(|rgba| Bitmap {
            rgba,
            width,
            height,
            origin_x: x,
            origin_y: y,
        })
    }
}

pub fn capture_virtual() -> Result<Bitmap, String> {
    let (x, y, w_, h_) = virtual_bounds();
    capture_region(x, y, w_, h_)
}

/// Capture a specific monitor (1-based). 0 or out of range -> primary/virtual.
pub fn capture_monitor(index: usize) -> Result<Bitmap, String> {
    let rects = monitor_rects();
    if index == 0 || index > rects.len() {
        if let Some((x, y, w_, h_, _)) = rects.iter().find(|r| r.4).copied() {
            return capture_region(x, y, w_, h_);
        }
        return capture_virtual();
    }
    let (x, y, w_, h_, _) = rects[index - 1];
    capture_region(x, y, w_, h_)
}

unsafe fn read_dibits(hdc: HDC, bmp: HBITMAP, width: i32, height: i32) -> Result<Vec<u8>, String> {
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; (width * height * 4) as usize];
    let scanned = GetDIBits(
        hdc,
        bmp,
        0,
        height as u32,
        Some(buf.as_mut_ptr() as *mut _),
        &mut bmi,
        DIB_RGB_COLORS,
    );
    if scanned == 0 {
        return Err("GetDIBits failed".into());
    }
    for px in buf.chunks_exact_mut(4) {
        px.swap(0, 2); // BGRA -> RGBA
        px[3] = 255;
    }
    Ok(buf)
}

pub fn cursor_pos() -> (i32, i32) {
    let mut pt = windows::Win32::Foundation::POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    (pt.x, pt.y)
}

// ---------------------------------------------------------------------------
// Annotation: cursor crosshair, Set-of-Marks, grid (built-in 3x5 digit font)
// ---------------------------------------------------------------------------
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

fn put(img: &mut RgbaImage, x: i32, y: i32, c: Rgba<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, c);
    }
}

fn fill_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, c: Rgba<u8>) {
    for yy in y..y + h {
        for xx in x..x + w {
            put(img, xx, yy, c);
        }
    }
}

fn draw_rect_outline(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, c: Rgba<u8>, thick: i32) {
    for t in 0..thick {
        for xx in x..x + w {
            put(img, xx, y + t, c);
            put(img, xx, y + h - 1 - t, c);
        }
        for yy in y..y + h {
            put(img, x + t, yy, c);
            put(img, x + w - 1 - t, yy, c);
        }
    }
}

fn draw_label(img: &mut RgbaImage, x: i32, y: i32, n: i64, scale: i32, fg: Rgba<u8>, bg: Rgba<u8>) {
    let s = n.abs().to_string();
    let digit_w = 3 * scale;
    let gap = scale;
    let text_w = s.len() as i32 * digit_w + (s.len() as i32 - 1).max(0) * gap;
    let text_h = 5 * scale;
    let pad = scale;
    fill_rect(img, x, y, text_w + 2 * pad, text_h + 2 * pad, bg);
    let mut cx = x + pad;
    for ch in s.chars() {
        let d = ch as usize - '0' as usize;
        let glyph = &DIGITS[d];
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..3i32 {
                if bits & (1 << (2 - col)) != 0 {
                    fill_rect(
                        img,
                        cx + col * scale,
                        y + pad + row as i32 * scale,
                        scale,
                        scale,
                        fg,
                    );
                }
            }
        }
        cx += digit_w + gap;
    }
}

pub fn annotate_cursor(img: &mut RgbaImage, ix: i32, iy: i32) {
    let red = Rgba([255, 0, 0, 255]);
    let r = 10;
    for d in -r..=r {
        put(img, ix + d, iy, red);
        put(img, ix, iy + d, red);
    }
}

/// Set-of-Marks: numbered boxes. Each mark is (index, image x, y, w, h).
pub fn annotate_marks(img: &mut RgbaImage, marks: &[(i64, i32, i32, i32, i32)]) {
    const PALETTE: [[u8; 3]; 6] = [
        [255, 64, 64],
        [64, 160, 255],
        [60, 200, 90],
        [230, 160, 30],
        [190, 90, 230],
        [0, 190, 190],
    ];
    for (idx, x, y, w, h) in marks.iter().copied() {
        let p = PALETTE[(idx as usize) % PALETTE.len()];
        let color = Rgba([p[0], p[1], p[2], 255]);
        draw_rect_outline(img, x, y, w.max(1), h.max(1), color, 2);
        draw_label(img, x, (y - 9).max(0), idx, 2, Rgba([0, 0, 0, 255]), color);
    }
}

pub fn annotate_grid(img: &mut RgbaImage, step: i32) {
    let line = Rgba([255, 0, 0, 90]);
    let (w_, h_) = (img.width() as i32, img.height() as i32);
    let mut x = 0;
    while x < w_ {
        for yy in 0..h_ {
            put(img, x, yy, line);
        }
        draw_label(
            img,
            x + 1,
            1,
            x as i64,
            2,
            Rgba([255, 255, 0, 255]),
            Rgba([0, 0, 0, 200]),
        );
        x += step;
    }
    let mut y = 0;
    while y < h_ {
        for xx in 0..w_ {
            put(img, xx, y, line);
        }
        draw_label(
            img,
            1,
            y + 1,
            y as i64,
            2,
            Rgba([255, 255, 0, 255]),
            Rgba([0, 0, 0, 200]),
        );
        y += step;
    }
}

// ---------------------------------------------------------------------------
// Scale + encode
// ---------------------------------------------------------------------------
pub fn scale_to(img: RgbaImage, max_dim: u32) -> (RgbaImage, f64) {
    if max_dim == 0 {
        return (img, 1.0);
    }
    let longest = img.width().max(img.height());
    if longest <= max_dim {
        return (img, 1.0);
    }
    let scale = max_dim as f64 / longest as f64;
    let nw = (img.width() as f64 * scale).round().max(1.0) as u32;
    let nh = (img.height() as f64 * scale).round().max(1.0) as u32;
    let resized = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
    (resized, scale)
}

/// Encode an RGBA image as base64 (no data: prefix) + mime type.
pub fn encode(img: &RgbaImage, fmt: &str, quality: u8) -> Result<(String, String), String> {
    let mut out: Vec<u8> = Vec::new();
    let mime = if fmt.eq_ignore_ascii_case("png") {
        image::DynamicImage::ImageRgba8(img.clone())
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .map_err(|e| format!("png encode failed: {e}"))?;
        "image/png".to_string()
    } else {
        let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
        enc.encode_image(&rgb)
            .map_err(|e| format!("jpeg encode failed: {e}"))?;
        "image/jpeg".to_string()
    };
    Ok((base64::engine::general_purpose::STANDARD.encode(&out), mime))
}

pub fn bitmap_to_image(bm: &Bitmap) -> Option<RgbaImage> {
    RgbaImage::from_raw(bm.width as u32, bm.height as u32, bm.rgba.clone())
}

/// Save a captured bitmap to a PNG file on disk.
pub fn save_png(bm: &Bitmap, path: &str) -> Result<(), String> {
    let img = bitmap_to_image(bm).ok_or("failed to wrap pixels")?;
    img.save(path).map_err(|e| format!("save failed: {e}"))
}

/// A tiny 32x32 grayscale fingerprint of a bitmap, for cheap screen-change diffs.
pub fn signature(bm: &Bitmap) -> Vec<u8> {
    let Some(img) = bitmap_to_image(bm) else {
        return Vec::new();
    };
    let thumb = image::imageops::resize(&img, 32, 32, image::imageops::FilterType::Triangle);
    thumb
        .pixels()
        .map(|p| {
            let [r, g, b, _] = p.0;
            ((r as u16 + g as u16 + b as u16) / 3) as u8
        })
        .collect()
}

/// Fraction of differing samples between two equal-length signatures (0.0..=1.0).
pub fn signature_diff(a: &[u8], b: &[u8]) -> f64 {
    if a.is_empty() || a.len() != b.len() {
        return 1.0;
    }
    let changed = a
        .iter()
        .zip(b)
        .filter(|(x, y)| (**x as i16 - **y as i16).abs() > 8)
        .count();
    changed as f64 / a.len() as f64
}
