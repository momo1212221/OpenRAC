// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
//! Offline texture post-processing used after the external neural upscaler.
//!
//! Upscalers can mistake one-pixel stars, windows and specular dots for noise. This module keeps
//! those source details and adds controlled micro-contrast without doing any work at game runtime.

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default)]
pub struct DetailStats {
    pub microdetails: usize,
    pub stars: usize,
}

struct RgbaImage {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

pub fn postprocess(
    source: &Path,
    upscaled: &Path,
    mode: &str,
    preserve_microdetail: bool,
    preserve_stars: bool,
) -> Result<DetailStats, String> {
    let source = read_rgba(source)?;
    let mut output = read_rgba(upscaled)?;
    if source.width == 0 || source.height == 0 || output.width < source.width || output.height < source.height {
        return Ok(DetailStats::default());
    }

    let (residual_strength, sharpen_strength) = match mode {
        "faithful" => (0.28, 0.04),
        "balanced" => (0.44, 0.16),
        "enhanced" => (0.56, 0.30),
        _ => return Err(format!("unknown AI texture mode: {mode}")),
    };

    let mut stats = DetailStats::default();
    if preserve_microdetail || preserve_stars {
        restore_source_detail(
            &source,
            &mut output,
            preserve_microdetail,
            preserve_stars,
            residual_strength,
            &mut stats,
        );
    }
    if sharpen_strength > 0.0 {
        sharpen(&mut output, sharpen_strength);
    }
    write_rgba(upscaled, &output)?;
    Ok(stats)
}

fn read_rgba(path: &Path) -> Result<RgbaImage, String> {
    let file = File::open(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| format!("could not decode {}: {e}", path.display()))?;
    let size = reader.output_buffer_size().ok_or_else(|| format!("PNG is too large: {}", path.display()))?;
    let mut raw = vec![0; size];
    let info = reader.next_frame(&mut raw).map_err(|e| format!("could not decode {}: {e}", path.display()))?;
    let bytes = &raw[..info.buffer_size()];
    let mut pixels = Vec::with_capacity(info.width as usize * info.height as usize * 4);

    match info.color_type {
        png::ColorType::Rgba => pixels.extend_from_slice(bytes),
        png::ColorType::Rgb => {
            for p in bytes.chunks_exact(3) {
                pixels.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for &v in bytes {
                pixels.extend_from_slice(&[v, v, v, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for p in bytes.chunks_exact(2) {
                pixels.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
            }
        }
        png::ColorType::Indexed => return Err(format!("indexed PNG was not expanded: {}", path.display())),
    }

    Ok(RgbaImage { width: info.width as usize, height: info.height as usize, pixels })
}

fn write_rgba(path: &Path, image: &RgbaImage) -> Result<(), String> {
    let temp = temp_png(path);
    let file = File::create(&temp).map_err(|e| format!("could not write {}: {e}", temp.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), image.width as u32, image.height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    {
        let mut writer = encoder.write_header().map_err(|e| format!("could not encode {}: {e}", temp.display()))?;
        writer.write_image_data(&image.pixels).map_err(|e| format!("could not encode {}: {e}", temp.display()))?;
    }
    std::fs::rename(&temp, path).map_err(|e| format!("could not replace {}: {e}", path.display()))
}

fn temp_png(path: &Path) -> PathBuf {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("texture");
    path.with_file_name(format!("{stem}.openrac-detail.tmp.png"))
}

fn luma(pixel: &[u8]) -> i32 {
    (54 * pixel[0] as i32 + 183 * pixel[1] as i32 + 19 * pixel[2] as i32) >> 8
}

fn pixel(image: &RgbaImage, x: usize, y: usize) -> &[u8] {
    let at = (y * image.width + x) * 4;
    &image.pixels[at..at + 4]
}

fn local_mean(image: &RgbaImage, x: usize, y: usize) -> ([i32; 3], i32, usize) {
    let mut rgb = [0i32; 3];
    let mut count = 0i32;
    let mut close_bright = 0usize;
    let center = luma(pixel(image, x, y));

    for yy in y.saturating_sub(1)..=(y + 1).min(image.height - 1) {
        for xx in x.saturating_sub(1)..=(x + 1).min(image.width - 1) {
            if xx == x && yy == y {
                continue;
            }

            let p = pixel(image, xx, yy);
            if p[3] < 16 {
                continue;
            }
            rgb[0] += p[0] as i32;
            rgb[1] += p[1] as i32;
            rgb[2] += p[2] as i32;
            count += 1;
            if luma(p) >= center - 24 {
                close_bright += 1;
            }
        }
    }

    if count == 0 {
        return ([0, 0, 0], 0, close_bright);
    }
    for channel in &mut rgb {
        *channel /= count;
    }
    let mean_luma = (54 * rgb[0] + 183 * rgb[1] + 19 * rgb[2]) >> 8;
    (rgb, mean_luma, close_bright)
}

fn restore_source_detail(
    source: &RgbaImage,
    output: &mut RgbaImage,
    preserve_microdetail: bool,
    preserve_stars: bool,
    strength: f32,
    stats: &mut DetailStats,
) {
    for y in 0..source.height {
        for x in 0..source.width {
            let p = pixel(source, x, y);
            if p[3] < 16 {
                continue;
            }
            let (mean, mean_luma, close_bright) = local_mean(source, x, y);
            let center_luma = luma(p);
            let contrast = center_luma - mean_luma;
            let star = preserve_stars && center_luma >= 150 && mean_luma <= 112 && contrast >= 52 && close_bright <= 2;

            if star {
                restore_star(output, source, x, y, p);
                stats.stars += 1;
                continue;
            }
            if preserve_microdetail && contrast.abs() >= 24 {
                let residual = [p[0] as i32 - mean[0], p[1] as i32 - mean[1], p[2] as i32 - mean[2]];
                add_residual(output, source, x, y, residual, strength);
                stats.microdetails += 1;
            }
        }
    }
}

fn mapped_center(output: &RgbaImage, source: &RgbaImage, x: usize, y: usize) -> (usize, usize) {
    let ox = (((x * 2 + 1) * output.width) / (source.width * 2)).min(output.width - 1);
    let oy = (((y * 2 + 1) * output.height) / (source.height * 2)).min(output.height - 1);
    (ox, oy)
}

fn center_block(output: &RgbaImage, source: &RgbaImage, x: usize, y: usize) -> Vec<(usize, usize)> {
    let (ox, oy) = mapped_center(output, source, x, y);
    let scale = (output.width / source.width).min(output.height / source.height);
    if scale >= 3 {
        vec![(ox, oy), ((ox + 1).min(output.width - 1), oy), (ox, (oy + 1).min(output.height - 1))]
    } else {
        vec![(ox, oy)]
    }
}

fn restore_star(output: &mut RgbaImage, source: &RgbaImage, x: usize, y: usize, original: &[u8]) {
    for (ox, oy) in center_block(output, source, x, y) {
        let at = (oy * output.width + ox) * 4;
        for channel in 0..3 {
            output.pixels[at + channel] = output.pixels[at + channel].max(original[channel]);
        }
        output.pixels[at + 3] = output.pixels[at + 3].max(original[3]);
    }
}

fn add_residual(output: &mut RgbaImage, source: &RgbaImage, x: usize, y: usize, residual: [i32; 3], strength: f32) {
    for (ox, oy) in center_block(output, source, x, y) {
        let at = (oy * output.width + ox) * 4;
        for channel in 0..3 {
            let add = residual[channel] as f32 * strength;
            output.pixels[at + channel] = (output.pixels[at + channel] as f32 + add).round().clamp(0.0, 255.0) as u8;
        }
    }
}

fn sharpen(image: &mut RgbaImage, strength: f32) {
    if image.width < 3 || image.height < 3 || strength <= 0.0 {
        return;
    }
    let original = image.pixels.clone();
    for y in 1..image.height - 1 {
        for x in 1..image.width - 1 {
            let at = (y * image.width + x) * 4;
            if original[at + 3] < 16 {
                continue;
            }

            for channel in 0..3 {
                let left = original[at - 4 + channel] as f32;
                let right = original[at + 4 + channel] as f32;
                let up = original[at - image.width * 4 + channel] as f32;
                let down = original[at + image.width * 4 + channel] as f32;
                let mean = (left + right + up + down) * 0.25;
                let center = original[at + channel] as f32;
                image.pixels[at + channel] = (center + (center - mean) * strength).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: usize, height: usize, rgb: [u8; 3]) -> RgbaImage {
        let mut pixels = Vec::with_capacity(width * height * 4);
        for _ in 0..width * height {
            pixels.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
        RgbaImage { width, height, pixels }
    }

    fn temp(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("openrac-texture-ai-{name}-{stamp}.png"))
    }

    #[test]
    fn restores_a_star_the_upscaler_removed() {
        let source_path = temp("source-star");
        let output_path = temp("output-star");
        let mut source = image(3, 3, [8, 8, 12]);
        let center = (1 * 3 + 1) * 4;
        source.pixels[center..center + 4].copy_from_slice(&[240, 235, 220, 255]);
        write_rgba(&source_path, &source).unwrap();
        write_rgba(&output_path, &image(12, 12, [12, 12, 18])).unwrap();

        let stats = postprocess(&source_path, &output_path, "faithful", false, true).unwrap();
        let output = read_rgba(&output_path).unwrap();
        let (x, y) = mapped_center(&output, &source, 1, 1);
        assert_eq!(stats.stars, 1);
        assert!(luma(pixel(&output, x, y)) >= 220);

        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(output_path);
    }

    #[test]
    fn restores_microcontrast_without_turning_it_into_a_star() {
        let source_path = temp("source-detail");
        let output_path = temp("output-detail");
        let mut source = image(3, 3, [100, 100, 100]);
        let center = (1 * 3 + 1) * 4;
        source.pixels[center..center + 4].copy_from_slice(&[150, 130, 110, 255]);
        write_rgba(&source_path, &source).unwrap();
        write_rgba(&output_path, &image(12, 12, [112, 112, 112])).unwrap();

        let stats = postprocess(&source_path, &output_path, "balanced", true, false).unwrap();
        let output = read_rgba(&output_path).unwrap();
        let (x, y) = mapped_center(&output, &source, 1, 1);
        assert!(stats.microdetails >= 1);
        assert!(pixel(&output, x, y)[0] > 112);

        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(output_path);
    }
}
