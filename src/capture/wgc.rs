use crate::capture::Bitmap;
use crate::winutil;
use std::cell::RefCell;
use std::time::{Duration, Instant};
use windows::core::Interface;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureDirtyRegionMode,
    GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

thread_local! {
    static DEVICE: RefCell<Option<(ID3D11Device, ID3D11DeviceContext, IDirect3DDevice)>> =
        const { RefCell::new(None) };
    static STAGING: RefCell<Option<(u32, u32, ID3D11Texture2D)>> = const { RefCell::new(None) };
}

fn create_d3d_device() -> Result<(ID3D11Device, ID3D11DeviceContext), String> {
    for driver in [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP] {
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        let hr = unsafe {
            D3D11CreateDevice(
                None,
                driver,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        };
        if hr.is_ok() {
            if let (Some(d), Some(c)) = (device, context) {
                return Ok((d, c));
            }
        }
    }
    Err("D3D11CreateDevice failed".into())
}

fn device() -> Result<(ID3D11Device, ID3D11DeviceContext, IDirect3DDevice), String> {
    DEVICE.with(|cell| {
        if let Some(t) = cell.borrow().as_ref() {
            return Ok(t.clone());
        }
        let (dev, ctx) = create_d3d_device()?;
        let dxgi: IDXGIDevice = dev.cast().map_err(|e| format!("IDXGIDevice cast: {e}"))?;
        let inspectable = unsafe {
            CreateDirect3D11DeviceFromDXGIDevice(&dxgi)
                .map_err(|e| format!("CreateDirect3D11DeviceFromDXGIDevice: {e}"))?
        };
        let rt: IDirect3DDevice = inspectable
            .cast()
            .map_err(|e| format!("IDirect3DDevice cast: {e}"))?;
        let tuple = (dev, ctx, rt);
        *cell.borrow_mut() = Some(tuple.clone());
        Ok(tuple)
    })
}

pub fn capture_window_wgc(hwnd: HWND) -> Result<Bitmap, String> {
    if !GraphicsCaptureSession::IsSupported().unwrap_or(false) {
        return Err("WGC not supported on this OS".into());
    }
    let (dev, ctx, rt_device) = device()?;

    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
        .map_err(|e| format!("capture interop factory: {e}"))?;
    let item: GraphicsCaptureItem = unsafe {
        interop
            .CreateForWindow(hwnd)
            .map_err(|e| format!("CreateForWindow: {e}"))?
    };
    let size: SizeInt32 = item.Size().map_err(|e| format!("item size: {e}"))?;
    if size.Width <= 0 || size.Height <= 0 {
        return Err("capture item has zero size".into());
    }

    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &rt_device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        1,
        size,
    )
    .map_err(|e| format!("CreateFreeThreaded: {e}"))?;
    let session = pool
        .CreateCaptureSession(&item)
        .map_err(|e| format!("CreateCaptureSession: {e}"))?;
    let _ = session.SetIsCursorCaptureEnabled(false);
    let _ = session.SetIsBorderRequired(false);
    let _ = session.SetIncludeSecondaryWindows(true);
    let _ = session.SetDirtyRegionMode(GraphicsCaptureDirtyRegionMode::ReportAndRender);
    session
        .StartCapture()
        .map_err(|e| format!("StartCapture: {e}"))?;

    let frame = match wait_for_frame(&pool) {
        Ok(frame) => frame,
        Err(err) => {
            let _ = session.Close();
            let _ = pool.Close();
            return Err(err);
        }
    };

    let extracted = extract_pixels(&dev, &ctx, &frame, size);
    let _ = frame.Close();
    let _ = session.Close();
    let _ = pool.Close();
    let rgba = extracted?;

    let (left, top) = winutil::capture_origin_for_size(hwnd, size.Width, size.Height);
    Ok(Bitmap {
        rgba,
        width: size.Width,
        height: size.Height,
        origin_x: left,
        origin_y: top,
    })
}

fn wait_for_frame(pool: &Direct3D11CaptureFramePool) -> Result<Direct3D11CaptureFrame, String> {
    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut frame = loop {
        if let Ok(f) = pool.TryGetNextFrame() {
            if !f.as_raw().is_null() {
                break f;
            }
        }
        if Instant::now() > deadline {
            return Err("WGC produced no frame within timeout".into());
        }
        std::thread::sleep(Duration::from_millis(4));
    };
    loop {
        match pool.TryGetNextFrame() {
            Ok(next) if !next.as_raw().is_null() => {
                let _ = frame.Close();
                frame = next;
            }
            _ => break,
        }
    }
    Ok(frame)
}

fn extract_pixels(
    dev: &ID3D11Device,
    ctx: &ID3D11DeviceContext,
    frame: &windows::Graphics::Capture::Direct3D11CaptureFrame,
    size: SizeInt32,
) -> Result<Vec<u8>, String> {
    let surface = frame.Surface().map_err(|e| format!("frame surface: {e}"))?;
    let access: IDirect3DDxgiInterfaceAccess = surface
        .cast()
        .map_err(|e| format!("dxgi access cast: {e}"))?;
    let texture: ID3D11Texture2D = unsafe {
        access
            .GetInterface()
            .map_err(|e| format!("GetInterface: {e}"))?
    };

    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { texture.GetDesc(&mut desc) };
    let staging = staging_texture(dev, &desc)?;
    unsafe { ctx.CopyResource(&staging, &texture) };

    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe {
        ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .map_err(|e| format!("Map: {e}"))?;
    }

    let w = (size.Width as usize).min(desc.Width as usize);
    let h = (size.Height as usize).min(desc.Height as usize);
    let row_pitch = mapped.RowPitch as usize;
    let src = mapped.pData as *const u8;
    let mut rgba = vec![0u8; w * h * 4];
    unsafe {
        for y in 0..h {
            let srow = src.add(y * row_pitch);
            let drow_start = y * w * 4;
            std::ptr::copy_nonoverlapping(srow, rgba.as_mut_ptr().add(drow_start), w * 4);
        }
        ctx.Unmap(&staging, 0);
    }
    // BGRA -> RGBA.
    for px in rgba.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    Ok(rgba)
}

fn staging_texture(
    dev: &ID3D11Device,
    src_desc: &D3D11_TEXTURE2D_DESC,
) -> Result<ID3D11Texture2D, String> {
    STAGING.with(|cell| {
        if let Some((w, h, tex)) = cell.borrow().as_ref() {
            if *w == src_desc.Width && *h == src_desc.Height {
                return Ok(tex.clone());
            }
        }
        let mut desc = *src_desc;
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;
        let mut staging: Option<ID3D11Texture2D> = None;
        unsafe {
            dev.CreateTexture2D(&desc, None, Some(&mut staging))
                .map_err(|e| format!("CreateTexture2D(staging): {e}"))?;
        }
        let staging = staging.ok_or("no staging texture")?;
        *cell.borrow_mut() = Some((src_desc.Width, src_desc.Height, staging.clone()));
        Ok(staging)
    })
}
