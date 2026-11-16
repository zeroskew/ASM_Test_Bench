//! Generates the app icon: `assets/asm_test_bench.ico` (16..256 px), `assets/icon_256.png`
//! (window icon) and `assets/icon_64.png` (toolbar logo).
//!
//!     build.cmd run --example mkicon
//!
//! A microchip on the app's blue tile; on the chip, three syntax-coloured lines of code with
//! the debugger's yellow execution arrow on the current line and a red breakpoint dot. Every
//! size is rendered directly from signed distance fields, so small icons stay crisp; the
//! smallest sizes drop the fine details.

use std::path::Path;

type Rgba = [f32; 4];
type P = (f32, f32);

fn rgb(hex: u32) -> Rgba {
    [((hex >> 16) & 255) as f32 / 255.0, ((hex >> 8) & 255) as f32 / 255.0, (hex & 255) as f32 / 255.0, 1.0]
}

fn alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], c[3] * a]
}

/// Distance to a rounded rectangle (negative inside). Coordinates in icon units (0..1).
fn sd_round_rect(p: P, min: P, max: P, r: f32) -> f32 {
    let c = ((min.0 + max.0) * 0.5, (min.1 + max.1) * 0.5);
    let h = ((max.0 - min.0) * 0.5 - r, (max.1 - min.1) * 0.5 - r);
    let q = ((p.0 - c.0).abs() - h.0, (p.1 - c.1).abs() - h.1);
    let outside = (q.0.max(0.0).powi(2) + q.1.max(0.0).powi(2)).sqrt();
    outside + q.0.max(q.1).min(0.0) - r
}

fn sd_circle(p: P, c: P, r: f32) -> f32 {
    ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt() - r
}

/// Distance to a triangle (negative inside).
fn sd_triangle(p: P, a: P, b: P, c: P) -> f32 {
    let edge = |p: P, a: P, b: P| -> (f32, f32) {
        let (e, w) = ((b.0 - a.0, b.1 - a.1), (p.0 - a.0, p.1 - a.1));
        let h = ((w.0 * e.0 + w.1 * e.1) / (e.0 * e.0 + e.1 * e.1)).clamp(0.0, 1.0);
        let d = ((w.0 - e.0 * h).powi(2) + (w.1 - e.1 * h).powi(2)).sqrt();
        (d, e.0 * w.1 - e.1 * w.0)
    };
    let (d0, s0) = edge(p, a, b);
    let (d1, s1) = edge(p, b, c);
    let (d2, s2) = edge(p, c, a);
    let d = d0.min(d1).min(d2);
    let inside = (s0 >= 0.0 && s1 >= 0.0 && s2 >= 0.0) || (s0 <= 0.0 && s1 <= 0.0 && s2 <= 0.0);
    if inside {
        -d
    } else {
        d
    }
}

struct Canvas {
    size: usize,
    px: Vec<Rgba>,
}

impl Canvas {
    fn new(size: usize) -> Self {
        Self { size, px: vec![[0.0; 4]; size * size] }
    }
    /// Paints `color` wherever `sdf` (in icon units) is inside, with ~1 px anti-aliasing.
    fn fill(&mut self, color: Rgba, sdf: impl Fn(P) -> f32) {
        let n = self.size;
        for y in 0..n {
            for x in 0..n {
                let p = ((x as f32 + 0.5) / n as f32, (y as f32 + 0.5) / n as f32);
                let cov = (0.5 - sdf(p) * n as f32).clamp(0.0, 1.0) * color[3];
                if cov <= 0.0 {
                    continue;
                }
                let d = &mut self.px[y * n + x];
                let a = cov + d[3] * (1.0 - cov);
                for i in 0..3 {
                    d[i] = if a > 0.0 { (color[i] * cov + d[i] * d[3] * (1.0 - cov)) / a } else { 0.0 };
                }
                d[3] = a;
            }
        }
    }
    /// Vertical gradient inside `sdf`.
    fn fill_gradient(&mut self, top: Rgba, bottom: Rgba, sdf: impl Fn(P) -> f32) {
        let n = self.size;
        for y in 0..n {
            let t = y as f32 / (n - 1) as f32;
            let c: Rgba = [0, 1, 2, 3].map(|i| top[i] + (bottom[i] - top[i]) * t);
            let row = |p: P| if ((p.1 * n as f32) as usize) == y { sdf(p) } else { 1.0 };
            self.fill(c, row);
        }
    }
    /// Width in icon units, but at least `min_px` pixels.
    fn w(&self, units: f32, min_px: f32) -> f32 {
        units.max(min_px / self.size as f32)
    }
    fn to_rgba8(&self) -> Vec<u8> {
        self.px.iter().flat_map(|c| c.map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)).collect()
    }
}

fn render(size: usize) -> Vec<u8> {
    let mut cv = Canvas::new(size);
    let tiny = size <= 20;
    let small = size <= 32;

    // Tile: the app's accent blue, darker at the bottom.
    let (tmin, tmax, tr) = ((0.02, 0.02), (0.98, 0.98), 0.21);
    let tile = |p: P| sd_round_rect(p, tmin, tmax, tr);
    cv.fill_gradient(rgb(0x1d6fc4), rgb(0x0b2f5c), tile);
    if !tiny {
        let w = cv.w(0.010, 1.0);
        cv.fill(alpha(rgb(0xffffff), 0.22), |p| tile(p).abs() - w * 0.5);
    }

    // Chip pins (metal), 4 per side (3 on small icons).
    let (cmin, cmax) = if tiny { ((0.20, 0.20), (0.80, 0.80)) } else { ((0.23, 0.23), (0.77, 0.77)) };
    let pins: &[f32] = if small { &[0.32, 0.50, 0.68] } else { &[0.30, 0.43, 0.57, 0.70] };
    let pin_len = if tiny { 0.10 } else { 0.10 };
    let pin_w = cv.w(if small { 0.075 } else { 0.055 }, 1.0) * 0.5;
    let metal = rgb(0xc9d3e3);
    for &t in pins {
        let r = pin_w * 0.6;
        cv.fill(metal, |p| sd_round_rect(p, (cmin.0 - pin_len, t - pin_w), (cmin.0 + 0.02, t + pin_w), r));
        cv.fill(metal, |p| sd_round_rect(p, (cmax.0 - 0.02, t - pin_w), (cmax.0 + pin_len, t + pin_w), r));
        cv.fill(metal, |p| sd_round_rect(p, (t - pin_w, cmin.1 - pin_len), (t + pin_w, cmin.1 + 0.02), r));
        cv.fill(metal, |p| sd_round_rect(p, (t - pin_w, cmax.1 - 0.02), (t + pin_w, cmax.1 + pin_len), r));
    }

    // Chip body: dark, like the editor background, with a light rim.
    let cr = 0.07;
    let chip = |p: P| sd_round_rect(p, cmin, cmax, cr);
    cv.fill(rgb(0x000000), |p| chip((p.0, p.1 - 0.012)) - 0.004); // shadow
    cv.fill_gradient(rgb(0x23262d), rgb(0x16181c), chip);
    let rim = cv.w(0.012, 1.0);
    cv.fill(alpha(rgb(0x8fa3c0), 0.75), |p| chip(p).abs() - rim * 0.5);
    if !small {
        // Pin-1 notch.
        cv.fill(alpha(rgb(0x8fa3c0), 0.6), |p| sd_circle(p, (cmin.0 + 0.065, cmin.1 + 0.065), 0.016));
    }

    // Code on the chip: current line highlighted, with the execution arrow.
    let inner_l = cmin.0 + if tiny { 0.08 } else { 0.07 };
    let code_x = inner_l + if tiny { 0.14 } else { 0.12 };
    let line_h = cv.w(if small { 0.075 } else { 0.05 }, 1.6) * 0.5;
    let rows: Vec<(f32, f32, Rgba)> = if small {
        vec![(0.38, 0.30, rgb(0x569cd6)), (0.62, 0.22, rgb(0x4ec9b0))]
    } else {
        vec![(0.36, 0.30, rgb(0x569cd6)), (0.50, 0.21, rgb(0x4ec9b0)), (0.64, 0.26, rgb(0xb5cea8))]
    };
    let current = if small { 1 } else { 1 };
    let cur_y = rows[current].0;
    // Highlight band across the chip for the current line.
    let band_h = line_h + if small { 0.035 } else { 0.03 };
    cv.fill(alpha(rgb(0xffc850), 0.20), |p| chip(p).max((p.1 - cur_y).abs() - band_h));
    for (i, &(y, len, c)) in rows.iter().enumerate() {
        let c = if i == current { c } else { alpha(c, 0.85) };
        cv.fill(c, |p| sd_round_rect(p, (code_x, y - line_h), (code_x + len, y + line_h), line_h));
        if !small && i == current {
            // Operand after the mnemonic: a second, shorter token.
            cv.fill(rgb(0xdcdcaa), |p| sd_round_rect(p, (code_x + len + 0.04, y - line_h), (code_x + len + 0.10, y + line_h), line_h));
        }
    }
    // Execution arrow (yellow), pointing at the current line.
    let ah = if small { 0.085 } else { 0.06 };
    let ax = inner_l - 0.005;
    cv.fill(rgb(0xffc94a), |p| sd_triangle(p, (ax, cur_y - ah), (ax + ah * 1.35, cur_y), (ax, cur_y + ah)) - 0.006);
    // Breakpoint (red dot) on the first line.
    if !tiny {
        let by = rows[0].0;
        let br = if small { 0.055 } else { 0.042 };
        cv.fill(rgb(0xe51400), |p| sd_circle(p, (ax + 0.04, by), br));
        if !small {
            cv.fill(alpha(rgb(0xffffff), 0.35), |p| sd_circle(p, (ax + 0.03, by - 0.012), br * 0.35));
        }
    }
    cv.to_rgba8()
}

fn png(size: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let enc = image::codecs::png::PngEncoder::new(&mut out);
    image::ImageEncoder::write_image(enc, &render(size), size as u32, size as u32, image::ExtendedColorType::Rgba8).unwrap();
    out
}

fn main() {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    let sizes = [16usize, 20, 24, 32, 40, 48, 64, 128, 256];
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| png(s)).collect();
    // ICO: header, one 16-byte directory entry per image, then the PNG data.
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len();
    for (s, data) in sizes.iter().zip(&images) {
        let b = if *s >= 256 { 0 } else { *s as u8 };
        ico.extend_from_slice(&[b, b, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(data.len() as u32).to_le_bytes());
        ico.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += data.len();
    }
    for data in &images {
        ico.extend_from_slice(data);
    }
    std::fs::write(assets.join("asm_test_bench.ico"), &ico).unwrap();
    std::fs::write(assets.join("icon_256.png"), images.last().unwrap()).unwrap();
    std::fs::write(assets.join("icon_64.png"), &images[6]).unwrap();
    // Preview sheet of all sizes on light and dark backgrounds, for checking by eye.
    let w: usize = sizes.iter().sum::<usize>() + 12 * sizes.len() + 12;
    let mut sheet = image::RgbaImage::from_pixel(w as u32, 2 * 268, image::Rgba([40, 40, 44, 255]));
    for y in 268..2 * 268 {
        for x in 0..w as u32 {
            sheet.put_pixel(x, y as u32, image::Rgba([235, 236, 240, 255]));
        }
    }
    for row in 0..2 {
        let mut x = 12;
        for &s in &sizes {
            let img = image::RgbaImage::from_raw(s as u32, s as u32, render(s)).unwrap();
            image::imageops::overlay(&mut sheet, &img, x as i64, (row * 268 + 6) as i64);
            x += s + 12;
        }
    }
    let sheet_path = std::env::temp_dir().join("asm_test_bench_icon_sheet.png");
    sheet.save(&sheet_path).unwrap();
    println!("wrote {} ({} bytes); preview: {}", assets.join("asm_test_bench.ico").display(), ico.len(), sheet_path.display());
}
