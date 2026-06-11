pub mod desktop;
pub mod redact;
pub mod wgc;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetWindowDC,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};

use crate::winutil;

/// A captured image in RGBA8 with its physical screen origin.
pub struct Bitmap {
    pub rgba: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub origin_x: i32,
    pub origin_y: i32,
}

const PW_RENDERFULLCONTENT: u32 = 0x00000002;

/// Capture a window, preferring the best available backend.
///
/// Windows.Graphics.Capture handles UWP / DirectComposition / occluded windows;
/// GDI `PrintWindow` is the fallback for older OSes or when WGC is unavailable.
pub fn capture_window(hwnd: HWND) -> Result<Bitmap, String> {
    match self::wgc::capture_window_wgc(hwnd) {
        Ok(bm) => Ok(bm),
        Err(e) => {
            eprintln!("opencontrol: WGC capture failed ({e}); falling back to GDI");
            capture_window_gdi(hwnd)
        }
    }
}

/// Capture a window's pixels via `PrintWindow`.
pub fn capture_window_gdi(hwnd: HWND) -> Result<Bitmap, String> {
    let (left, top, right, bottom) = winutil::window_rect(hwnd).ok_or("window has no rect")?;
    let width = right - left;
    let height = bottom - top;
    if width <= 0 || height <= 0 {
        return Err("window has zero size".into());
    }

    unsafe {
        let hwnd_dc: HDC = GetWindowDC(hwnd);
        if hwnd_dc.0.is_null() {
            return Err("GetWindowDC failed".into());
        }
        let mem_dc = CreateCompatibleDC(hwnd_dc);
        let mem_hdc = HDC(mem_dc.0);
        let bmp: HBITMAP = CreateCompatibleBitmap(hwnd_dc, width, height);
        let old = SelectObject(mem_hdc, HGDIOBJ(bmp.0));

        // PW_RENDERFULLCONTENT renders DirectComposition surfaces (browsers etc.).
        let mut ok = PrintWindow(hwnd, mem_hdc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool();
        if !ok {
            ok = PrintWindow(hwnd, mem_hdc, PRINT_WINDOW_FLAGS(0)).as_bool();
        }

        let result = if ok {
            read_dibits(mem_hdc, bmp, width, height)
        } else {
            Err("PrintWindow failed".into())
        };

        SelectObject(mem_hdc, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem_hdc);
        ReleaseDC(hwnd, hwnd_dc);

        result.map(|rgba| Bitmap {
            rgba,
            width,
            height,
            origin_x: left,
            origin_y: top,
        })
    }
}

unsafe fn read_dibits(hdc: HDC, bmp: HBITMAP, width: i32, height: i32) -> Result<Vec<u8>, String> {
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0, // BI_RGB
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
    // BGRA (from GDI) -> RGBA, and force opaque alpha (GDI leaves it 0).
    for px in buf.chunks_exact_mut(4) {
        px.swap(0, 2);
        px[3] = 255;
    }
    Ok(buf)
}
