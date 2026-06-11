use crate::blocklist::{Blocklist, RedactMode};
use crate::capture::Bitmap;
use crate::winutil;

/// An axis-aligned rectangle in pixels: top-left `(x, y)` plus size `(w, h)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl PixelRect {
    fn from_ltrb(l: i32, t: i32, r: i32, b: i32) -> PixelRect {
        PixelRect {
            x: l,
            y: t,
            w: (r - l).max(0),
            h: (b - t).max(0),
        }
    }
    fn right(&self) -> i32 {
        self.x + self.w
    }
    fn bottom(&self) -> i32 {
        self.y + self.h
    }
    fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }
    fn intersect(&self, o: &PixelRect) -> Option<PixelRect> {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        if r > x && b > y {
            Some(PixelRect {
                x,
                y,
                w: r - x,
                h: b - y,
            })
        } else {
            None
        }
    }
}

/// Result of a redaction pass over one bitmap.
#[derive(Debug, Default, Clone, Copy)]
pub struct RedactOutcome {
    /// Number of filled/blurred sub-rectangles drawn.
    pub redacted: usize,
    /// True when coverage hit the full-frame threshold and the whole frame was blanked.
    pub fully_redacted: bool,
}

// System/phantom windows that should never carve "holes" out of a redaction.
const NON_OCCLUDING_CLASSES: [&str; 7] = [
    "ime",
    "msctfime ui",
    "default ime",
    "cicerouiwndframe",
    "tooltips_class32",
    "sysshadow",
    "narratorhelperwindow",
];

// Windows smaller than this on either side don't count as occluders.
const MIN_OCCLUDER_SIDE: i32 = 32;

// Blank the entire frame once redaction covers at least this fraction of it.
const FULL_FRAME_PERCENT: i64 = 95;

struct EnumWin {
    hwnd: windows::Win32::Foundation::HWND,
    info: crate::blocklist::WindowInfo,
    blocked: bool,
    cloaked: bool,
}

/// Redact every blocked window visible in `bm`, in place.
///
/// Walks top-level windows in z-order, and for each blocked window fills (or
/// blurs) the parts of it that aren't covered by windows above it, plus its
/// transient popups (menus/dropdowns). Returns early and cheaply when the
/// blocklist is empty. When `fail_closed` is set and the window list can't be
/// obtained, the capture is refused rather than risk leaking a blocked window.
pub fn apply_to_bitmap(bm: &mut Bitmap, bl: &Blocklist) -> Result<RedactOutcome, String> {
    if bl.is_empty() || bm.width <= 0 || bm.height <= 0 {
        return Ok(RedactOutcome::default());
    }

    let wins = enumerate_windows(bl);
    if wins.is_empty() {
        if bl.fail_closed {
            return Err("screen redaction unavailable: window enumeration returned nothing".into());
        }
        return Ok(RedactOutcome::default());
    }

    // Credible occluders (non-blocked, visible, real windows) with their z-index.
    let occluders: Vec<(usize, PixelRect)> = wins
        .iter()
        .enumerate()
        .filter_map(|(z, w)| {
            if w.blocked || w.cloaked {
                return None;
            }
            let (l, t, r, b) = w.info.rect?;
            let pr = PixelRect::from_ltrb(l, t, r, b);
            if is_credible_occluder(&w.info.class_name, &pr) {
                Some((z, pr))
            } else {
                None
            }
        })
        .collect();

    // Redaction targets in SCREEN space: (rect, mode, z-index for occlusion).
    // Popups use `None` (treated as topmost — nothing occludes them).
    let mut targets: Vec<(PixelRect, RedactMode, Option<usize>)> = Vec::new();
    for (z, w) in wins.iter().enumerate() {
        if !w.blocked || w.cloaked {
            continue;
        }
        let mode = bl.redact_mode_for(&w.info).unwrap_or(bl.default_mode);
        if let Some((l, t, r, b)) = w.info.rect {
            targets.push((PixelRect::from_ltrb(l, t, r, b), mode, Some(z)));
        }
        for ph in winutil::transient_popups(w.hwnd) {
            if let Some((l, t, r, b)) = winutil::window_rect(ph) {
                targets.push((PixelRect::from_ltrb(l, t, r, b), mode, None));
            }
        }
    }

    // Compute visible (non-occluded) sub-rects, mapped into bitmap space.
    let frame = PixelRect {
        x: 0,
        y: 0,
        w: bm.width,
        h: bm.height,
    };
    let mut rects: Vec<(PixelRect, RedactMode)> = Vec::new();
    for (screen_rect, mode, zt) in targets {
        let occ: Vec<PixelRect> = match zt {
            Some(zt) => occluders
                .iter()
                .filter(|(z, _)| *z < zt)
                .map(|(_, r)| *r)
                .collect(),
            None => Vec::new(),
        };
        for v in subtract_occluders(screen_rect, &occ) {
            if let Some(c) = to_bitmap_rect(&v, bm.origin_x, bm.origin_y, &frame) {
                rects.push((c, mode));
            }
        }
    }

    if rects.is_empty() {
        return Ok(RedactOutcome::default());
    }

    // Full-frame guard: if nearly everything is blocked, blank the whole frame.
    let all: Vec<PixelRect> = rects.iter().map(|(r, _)| *r).collect();
    let total = bm.width as i64 * bm.height as i64;
    if total > 0 && union_area(&all) * 100 >= total * FULL_FRAME_PERCENT {
        fill_solid(bm, &frame, [0, 0, 0]);
        return Ok(RedactOutcome {
            redacted: rects.len(),
            fully_redacted: true,
        });
    }

    let count = rects.len();
    for (r, mode) in rects {
        match mode {
            RedactMode::Solid(rgb) => fill_solid(bm, &r, rgb),
            RedactMode::Blur(sigma) => blur_region(bm, &r, sigma),
        }
    }
    Ok(RedactOutcome {
        redacted: count,
        fully_redacted: false,
    })
}

fn enumerate_windows(bl: &Blocklist) -> Vec<EnumWin> {
    winutil::enum_top_level()
        .into_iter()
        .map(|hwnd| {
            let info = winutil::window_info(hwnd);
            let blocked = bl.is_blocked(&info);
            let cloaked = winutil::is_cloaked(hwnd);
            EnumWin {
                hwnd,
                info,
                blocked,
                cloaked,
            }
        })
        .collect()
}

fn is_credible_occluder(class: &str, r: &PixelRect) -> bool {
    if r.w < MIN_OCCLUDER_SIDE || r.h < MIN_OCCLUDER_SIDE {
        return false;
    }
    let c = class.to_lowercase();
    !NON_OCCLUDING_CLASSES.contains(&c.as_str())
}

/// Translate a screen-space rect into bitmap pixels and clip it to the frame.
fn to_bitmap_rect(
    screen: &PixelRect,
    origin_x: i32,
    origin_y: i32,
    frame: &PixelRect,
) -> Option<PixelRect> {
    let b = PixelRect {
        x: screen.x - origin_x,
        y: screen.y - origin_y,
        w: screen.w,
        h: screen.h,
    };
    b.intersect(frame).filter(|r| !r.is_empty())
}

// ---- pure geometry ---------------------------------------------------------

/// `target` minus its intersection with `hole`, as up to four sub-rectangles.
fn subtract_hole(target: &PixelRect, hole: &PixelRect) -> Vec<PixelRect> {
    let inter = match target.intersect(hole) {
        Some(i) => i,
        None => return vec![*target],
    };
    let mut out = Vec::new();
    if inter.y > target.y {
        out.push(PixelRect {
            x: target.x,
            y: target.y,
            w: target.w,
            h: inter.y - target.y,
        });
    }
    if inter.bottom() < target.bottom() {
        out.push(PixelRect {
            x: target.x,
            y: inter.bottom(),
            w: target.w,
            h: target.bottom() - inter.bottom(),
        });
    }
    if inter.x > target.x {
        out.push(PixelRect {
            x: target.x,
            y: inter.y,
            w: inter.x - target.x,
            h: inter.h,
        });
    }
    if inter.right() < target.right() {
        out.push(PixelRect {
            x: inter.right(),
            y: inter.y,
            w: target.right() - inter.right(),
            h: inter.h,
        });
    }
    out
}

/// `target` minus the union of all `occluders`, as a set of non-overlapping rects.
fn subtract_occluders(target: PixelRect, occluders: &[PixelRect]) -> Vec<PixelRect> {
    if target.is_empty() {
        return Vec::new();
    }
    let mut regions = vec![target];
    for occ in occluders {
        let mut next = Vec::new();
        for r in &regions {
            next.extend(subtract_hole(r, occ));
        }
        regions = next;
        if regions.is_empty() {
            break;
        }
    }
    regions
}

/// Exact area of the union of `rects` (handles overlap) via coordinate compression.
fn union_area(rects: &[PixelRect]) -> i64 {
    let rects: Vec<&PixelRect> = rects.iter().filter(|r| !r.is_empty()).collect();
    if rects.is_empty() {
        return 0;
    }
    let mut xs: Vec<i32> = Vec::with_capacity(rects.len() * 2);
    for r in &rects {
        xs.push(r.x);
        xs.push(r.right());
    }
    xs.sort_unstable();
    xs.dedup();

    let mut area: i64 = 0;
    for win in xs.windows(2) {
        let (x0, x1) = (win[0], win[1]);
        let slab_w = (x1 - x0) as i64;
        if slab_w <= 0 {
            continue;
        }
        let mut ys: Vec<(i32, i32)> = rects
            .iter()
            .filter(|r| r.x <= x0 && r.right() >= x1)
            .map(|r| (r.y, r.bottom()))
            .collect();
        if ys.is_empty() {
            continue;
        }
        ys.sort_unstable();
        let mut covered: i64 = 0;
        let (mut cs, mut ce) = ys[0];
        for &(s, e) in &ys[1..] {
            if s > ce {
                covered += (ce - cs) as i64;
                cs = s;
                ce = e;
            } else if e > ce {
                ce = e;
            }
        }
        covered += (ce - cs) as i64;
        area += slab_w * covered;
    }
    area
}

// ---- pixel operations (operate directly on the RGBA buffer) ----------------

/// Fill `r` (already clipped to the frame) with an opaque solid color.
fn fill_solid(bm: &mut Bitmap, r: &PixelRect, rgb: [u8; 3]) {
    let stride = bm.width as usize * 4;
    for yy in r.y..r.bottom() {
        let row = yy as usize * stride;
        for xx in r.x..r.right() {
            let i = row + xx as usize * 4;
            if i + 3 < bm.rgba.len() {
                bm.rgba[i] = rgb[0];
                bm.rgba[i + 1] = rgb[1];
                bm.rgba[i + 2] = rgb[2];
                bm.rgba[i + 3] = 255;
            }
        }
    }
}

/// Gaussian-blur `r` (already clipped to the frame) in place.
fn blur_region(bm: &mut Bitmap, r: &PixelRect, sigma: f32) {
    if r.is_empty() {
        return;
    }
    let stride = bm.width as usize * 4;
    let mut sub = image::RgbaImage::new(r.w as u32, r.h as u32);
    for yy in 0..r.h {
        for xx in 0..r.w {
            let si = (r.y + yy) as usize * stride + (r.x + xx) as usize * 4;
            if si + 3 < bm.rgba.len() {
                let px = image::Rgba([
                    bm.rgba[si],
                    bm.rgba[si + 1],
                    bm.rgba[si + 2],
                    bm.rgba[si + 3],
                ]);
                sub.put_pixel(xx as u32, yy as u32, px);
            }
        }
    }
    let blurred = image::imageops::blur(&sub, sigma.max(1.0));
    for yy in 0..r.h {
        for xx in 0..r.w {
            let p = blurred.get_pixel(xx as u32, yy as u32);
            let di = (r.y + yy) as usize * stride + (r.x + xx) as usize * 4;
            if di + 3 < bm.rgba.len() {
                bm.rgba[di] = p[0];
                bm.rgba[di + 1] = p[1];
                bm.rgba[di + 2] = p[2];
                bm.rgba[di + 3] = 255;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> PixelRect {
        PixelRect { x, y, w, h }
    }

    fn solid_bitmap(w: i32, h: i32, rgb: [u8; 3]) -> Bitmap {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
        Bitmap {
            rgba,
            width: w,
            height: h,
            origin_x: 0,
            origin_y: 0,
        }
    }

    fn pixel(bm: &Bitmap, x: i32, y: i32) -> [u8; 4] {
        let i = (y as usize * bm.width as usize + x as usize) * 4;
        [bm.rgba[i], bm.rgba[i + 1], bm.rgba[i + 2], bm.rgba[i + 3]]
    }

    #[test]
    fn intersect_basic() {
        assert_eq!(
            rect(0, 0, 10, 10).intersect(&rect(5, 5, 10, 10)),
            Some(rect(5, 5, 5, 5))
        );
        assert_eq!(rect(0, 0, 10, 10).intersect(&rect(20, 20, 5, 5)), None);
        assert_eq!(
            rect(0, 0, 10, 10).intersect(&rect(2, 2, 3, 3)),
            Some(rect(2, 2, 3, 3))
        );
    }

    #[test]
    fn subtract_hole_center_makes_four() {
        let parts = subtract_hole(&rect(0, 0, 30, 30), &rect(10, 10, 10, 10));
        assert_eq!(parts.len(), 4);
        // None of the parts overlap the hole.
        for p in &parts {
            assert!(p.intersect(&rect(10, 10, 10, 10)).is_none());
        }
        // Areas sum to the original minus the hole.
        let total: i32 = parts.iter().map(|p| p.w * p.h).sum();
        assert_eq!(total, 30 * 30 - 10 * 10);
    }

    #[test]
    fn subtract_hole_full_cover_empty() {
        assert!(subtract_hole(&rect(5, 5, 10, 10), &rect(0, 0, 100, 100)).is_empty());
    }

    #[test]
    fn subtract_hole_no_overlap_returns_original() {
        let parts = subtract_hole(&rect(0, 0, 10, 10), &rect(50, 50, 10, 10));
        assert_eq!(parts, vec![rect(0, 0, 10, 10)]);
    }

    #[test]
    fn subtract_occluders_multiple() {
        // Two occluders take the left and right thirds; middle band remains.
        let regions = subtract_occluders(
            rect(0, 0, 30, 10),
            &[rect(0, 0, 10, 10), rect(20, 0, 10, 10)],
        );
        let area: i32 = regions.iter().map(|r| r.w * r.h).sum();
        assert_eq!(area, 10 * 10);
        for r in &regions {
            assert!(r.intersect(&rect(0, 0, 10, 10)).is_none());
            assert!(r.intersect(&rect(20, 0, 10, 10)).is_none());
        }
    }

    #[test]
    fn union_area_disjoint_and_overlap() {
        assert_eq!(union_area(&[rect(0, 0, 10, 10), rect(20, 0, 10, 10)]), 200);
        // Overlapping squares: 2*100 - 25 overlap.
        assert_eq!(union_area(&[rect(0, 0, 10, 10), rect(5, 5, 10, 10)]), 175);
        // Fully contained.
        assert_eq!(union_area(&[rect(0, 0, 10, 10), rect(2, 2, 3, 3)]), 100);
    }

    #[test]
    fn to_bitmap_rect_translates_and_clips() {
        let frame = rect(0, 0, 100, 100);
        // Screen rect at (110,110) with origin (100,100) -> (10,10) in bitmap.
        assert_eq!(
            to_bitmap_rect(&rect(110, 110, 20, 20), 100, 100, &frame),
            Some(rect(10, 10, 20, 20))
        );
        // Partly outside the frame is clipped.
        assert_eq!(
            to_bitmap_rect(&rect(190, 190, 20, 20), 100, 100, &frame),
            Some(rect(90, 90, 10, 10))
        );
        // Entirely outside -> None.
        assert_eq!(
            to_bitmap_rect(&rect(300, 300, 20, 20), 100, 100, &frame),
            None
        );
    }

    #[test]
    fn fill_solid_blacks_out_region() {
        let mut bm = solid_bitmap(10, 10, [255, 255, 255]);
        fill_solid(&mut bm, &rect(2, 2, 3, 3), [0, 0, 0]);
        // Inside the rect is black, alpha forced opaque.
        assert_eq!(pixel(&bm, 2, 2), [0, 0, 0, 255]);
        assert_eq!(pixel(&bm, 4, 4), [0, 0, 0, 255]);
        // Outside is untouched.
        assert_eq!(pixel(&bm, 0, 0), [255, 255, 255, 255]);
        assert_eq!(pixel(&bm, 5, 5), [255, 255, 255, 255]);
    }

    #[test]
    fn blur_region_changes_pixels_and_keeps_alpha() {
        // Left half black, right half white; blur the seam.
        let mut bm = solid_bitmap(20, 8, [0, 0, 0]);
        for y in 0..8 {
            for x in 10..20 {
                let i = (y * 20 + x) as usize * 4;
                bm.rgba[i] = 255;
                bm.rgba[i + 1] = 255;
                bm.rgba[i + 2] = 255;
            }
        }
        blur_region(&mut bm, &rect(5, 0, 10, 8), 4.0);
        // A pixel right at the seam should no longer be pure black or white.
        let p = pixel(&bm, 10, 4);
        assert!(p[0] > 0 && p[0] < 255, "expected blended gray, got {:?}", p);
        assert_eq!(p[3], 255);
    }
}
