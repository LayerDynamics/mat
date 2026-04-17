//! Regenerate the demo images used by the `examples/*.md` files.
//!
//! Run with:
//!
//! ```text
//! cargo run --example gen_demo_images
//! ```
//!
//! The generated PNGs / JPGs land in `examples/assets/`. Every file is built
//! from deterministic pixel math — no external inputs, no random seeds — so
//! the output is byte-reproducible across platforms. That keeps diffs sane
//! when the demo set is regenerated.

use std::path::PathBuf;

use image::{ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};

fn assets_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("examples").join("assets")
}

/// Smooth HSV → RGB for the rainbow gradient. Hue in [0, 360), s/v in [0, 1].
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let h6 = h / 60.0;
    let x = c * (1.0 - ((h6 % 2.0) - 1.0).abs());
    let (r1, g1, b1) = match h6 as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [
        ((r1 + m) * 255.0).clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).clamp(0.0, 255.0) as u8,
    ]
}

fn gradient(path: &PathBuf) {
    let (w, h) = (400u32, 80u32);
    let mut img = RgbImage::new(w, h);
    for y in 0..h {
        let value = 0.55 + 0.45 * (y as f32 / h as f32);
        for x in 0..w {
            let hue = 360.0 * (x as f32 / w as f32);
            img.put_pixel(x, y, Rgb(hsv_to_rgb(hue, 0.85, value)));
        }
    }
    img.save_with_format(path, ImageFormat::Png).unwrap();
}

fn mosaic(path: &PathBuf) {
    // 6 × 4 grid of saturated tiles — reads well in both full-color and
    // half-block fallback because every tile is a single flat color.
    let (tile, cols, rows) = (48u32, 6u32, 4u32);
    let mut img = RgbImage::new(tile * cols, tile * rows);
    let palette: [[u8; 3]; 24] = [
        [239, 68, 68],
        [249, 115, 22],
        [234, 179, 8],
        [34, 197, 94],
        [20, 184, 166],
        [59, 130, 246],
        [99, 102, 241],
        [168, 85, 247],
        [236, 72, 153],
        [244, 63, 94],
        [251, 191, 36],
        [132, 204, 22],
        [16, 185, 129],
        [6, 182, 212],
        [14, 165, 233],
        [79, 70, 229],
        [139, 92, 246],
        [217, 70, 239],
        [244, 114, 182],
        [248, 113, 113],
        [253, 186, 116],
        [254, 240, 138],
        [190, 242, 100],
        [134, 239, 172],
    ];
    for row in 0..rows {
        for col in 0..cols {
            let c = palette[((row * cols + col) as usize) % palette.len()];
            for y in row * tile..(row + 1) * tile {
                for x in col * tile..(col + 1) * tile {
                    img.put_pixel(x, y, Rgb(c));
                }
            }
        }
    }
    img.save_with_format(path, ImageFormat::Png).unwrap();
}

fn chart(path: &PathBuf) {
    // Seven-bar chart with alternating hues and a dim baseline, 320 × 160.
    let (w, h, pad) = (320u32, 160u32, 12u32);
    let bg = Rgb([24u8, 24, 28]);
    let axis = Rgb([96u8, 96, 108]);
    let mut img = RgbImage::new(w, h);
    for p in img.pixels_mut() {
        *p = bg;
    }
    let heights = [52u32, 98, 74, 120, 62, 132, 88];
    let hues = [
        [96u8, 165, 250],
        [134, 239, 172],
        [249, 168, 212],
        [253, 224, 71],
        [192, 132, 252],
        [253, 186, 116],
        [110, 231, 183],
    ];
    let n = heights.len() as u32;
    let gutter = 8u32;
    let bar_w = (w - 2 * pad - gutter * (n - 1)) / n;
    for (i, &bh) in heights.iter().enumerate() {
        let x0 = pad + (i as u32) * (bar_w + gutter);
        let x1 = x0 + bar_w;
        let y0 = h - pad - bh;
        let y1 = h - pad;
        let color = Rgb(hues[i]);
        for y in y0..y1 {
            for x in x0..x1 {
                img.put_pixel(x, y, color);
            }
        }
    }
    // Baseline
    for x in pad..(w - pad) {
        img.put_pixel(x, h - pad, axis);
        img.put_pixel(x, h - pad - 1, axis);
    }
    img.save_with_format(path, ImageFormat::Png).unwrap();
}

fn logo(path: &PathBuf) {
    // 240 × 240 rounded-corner-ish "m" glyph on a dark panel. Pure pixel
    // math — three filled verticals + one bottom connector — so it's crisp
    // at any half-block downscale and recognizable at small sizes.
    let (side, margin) = (240u32, 24u32);
    let bg = Rgba([17u8, 24, 39, 255]);
    let fg = Rgba([56u8, 189, 248, 255]);
    let mut img = RgbaImage::new(side, side);
    for p in img.pixels_mut() {
        *p = bg;
    }
    let inner_w = side - 2 * margin;
    let stroke = inner_w / 8;
    // Three verticals: left, middle, right
    let tops = [margin, margin + inner_w / 3, margin];
    let lefts = [margin, margin + inner_w / 2 - stroke / 2, side - margin - stroke];
    let bottom = side - margin;
    for (t, l) in tops.iter().zip(lefts.iter()) {
        for y in *t..bottom {
            for x in *l..(*l + stroke) {
                if x < side && y < side {
                    img.put_pixel(x, y, fg);
                }
            }
        }
    }
    // Top connector between left and middle
    let top_y = margin;
    for y in top_y..(top_y + stroke) {
        for x in lefts[0]..(lefts[1] + stroke) {
            if x < side && y < side {
                img.put_pixel(x, y, fg);
            }
        }
    }
    // Middle cap between middle and right
    for y in tops[1]..(tops[1] + stroke) {
        for x in lefts[1]..(lefts[2] + stroke) {
            if x < side && y < side {
                img.put_pixel(x, y, fg);
            }
        }
    }
    img.save_with_format(path, ImageFormat::Png).unwrap();
}

fn photo(path: &PathBuf) {
    // A JPEG-encoded radial gradient so the demo exercises the `jpeg`
    // feature of the `image` crate alongside the PNG cases. 192 × 192.
    let side = 192u32;
    let mut img = RgbImage::new(side, side);
    let cx = side as f32 / 2.0;
    let cy = side as f32 / 2.0;
    let max_r = (cx * cx + cy * cy).sqrt();
    for y in 0..side {
        for x in 0..side {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let r = (dx * dx + dy * dy).sqrt() / max_r;
            let hue = 360.0 * (1.0 - r);
            let rgb = hsv_to_rgb(hue, 0.7, 0.9 - 0.3 * r);
            img.put_pixel(x, y, Rgb(rgb));
        }
    }
    img.save_with_format(path, ImageFormat::Jpeg).unwrap();
}

type Builder = fn(&PathBuf);

fn main() {
    let dir = assets_dir();
    std::fs::create_dir_all(&dir).unwrap();

    let targets: &[(&str, Builder)] = &[
        ("gradient.png", gradient),
        ("mosaic.png", mosaic),
        ("chart.png", chart),
        ("logo.png", logo),
        ("photo.jpg", photo),
    ];

    for (name, build) in targets {
        let path = dir.join(name);
        build(&path);
        let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        println!("wrote {} ({} bytes)", path.display(), bytes);
    }
}
