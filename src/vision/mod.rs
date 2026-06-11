pub mod ocr;
use crate::capture::Bitmap;
use image::RgbaImage;

fn to_gray(img: &RgbaImage) -> (Vec<f32>, u32, u32) {
    let (w, h) = img.dimensions();
    let mut g = vec![0f32; (w * h) as usize];
    for (i, px) in img.pixels().enumerate() {
        let [r, gr, b, _] = px.0;
        g[i] = 0.299 * r as f32 + 0.587 * gr as f32 + 0.114 * b as f32;
    }
    (g, w, h)
}

fn mean(v: &[f32]) -> f32 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f32>() / v.len() as f32
    }
}

/// Locate a template image inside a captured region via normalized
/// cross-correlation on grayscale. Coordinates in matches are absolute screen
/// pixels (region origin + offset). Bound the search region for speed.
pub fn find_image(
    haystack: &Bitmap,
    template_path: &str,
    threshold: f32,
    all_matches: bool,
) -> Result<serde_json::Value, String> {
    let scene = RgbaImage::from_raw(
        haystack.width as u32,
        haystack.height as u32,
        haystack.rgba.clone(),
    )
    .ok_or("failed to wrap captured pixels")?;
    let tmpl = image::open(template_path)
        .map_err(|e| format!("failed to open template '{template_path}': {e}"))?
        .to_rgba8();

    let (sg, sw, sh) = to_gray(&scene);
    let (tg, tw, th) = to_gray(&tmpl);
    if tw > sw || th > sh {
        return Err("template is larger than the search region".into());
    }

    // Keep naive NCC bounded; suggest a smaller region if the work is excessive.
    let positions = ((sw - tw + 1) as u64) * ((sh - th + 1) as u64);
    let work = positions * (tw as u64) * (th as u64);
    if work > 4_000_000_000 {
        return Err("search area too large; pass a smaller 'region'".into());
    }

    let tmean = mean(&tg);
    let mut tnorm = 0f32;
    let tdev: Vec<f32> = tg
        .iter()
        .map(|&v| {
            let d = v - tmean;
            tnorm += d * d;
            d
        })
        .collect();
    let tnorm = tnorm.sqrt().max(1e-6);

    let mut matches: Vec<(f32, i32, i32)> = Vec::new();
    for oy in 0..=(sh - th) {
        for ox in 0..=(sw - tw) {
            // window mean
            let mut wsum = 0f32;
            for ry in 0..th {
                let row = ((oy + ry) * sw + ox) as usize;
                for rx in 0..tw as usize {
                    wsum += sg[row + rx];
                }
            }
            let wmean = wsum / (tw * th) as f32;

            let mut dot = 0f32;
            let mut wnorm = 0f32;
            for ry in 0..th {
                let srow = ((oy + ry) * sw + ox) as usize;
                let trow = (ry * tw) as usize;
                for rx in 0..tw as usize {
                    let d = sg[srow + rx] - wmean;
                    dot += d * tdev[trow + rx];
                    wnorm += d * d;
                }
            }
            let score = dot / (tnorm * wnorm.sqrt().max(1e-6));
            if score >= threshold {
                matches.push((score, ox as i32, oy as i32));
                if !all_matches {
                    break;
                }
            }
        }
        if !all_matches && !matches.is_empty() {
            break;
        }
    }

    matches.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let out: Vec<serde_json::Value> = matches
        .iter()
        .take(if all_matches { 50 } else { 1 })
        .map(|(score, ox, oy)| {
            let sx = haystack.origin_x + ox;
            let sy = haystack.origin_y + oy;
            serde_json::json!({
                "score": (score * 1000.0).round() / 1000.0,
                "x": sx, "y": sy,
                "width": tw, "height": th,
                "center": { "x": sx + tw as i32 / 2, "y": sy + th as i32 / 2 }
            })
        })
        .collect();
    Ok(serde_json::json!({ "found": !out.is_empty(), "matches": out }))
}
