// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors

//! A game's texture pack (docs/port/TEXTURE_PACKS.md): pictures that replace the game's
//! textures, each a PNG named by the 16 hex digits of the texture it replaces, in any folders; an
//! animated one is a folder named so, holding numbered frames and an optional `animation.json`,
//! or a GIF named so.
//!
//! ```text
//! <data>/texture_packs/<id>/            the player's pack, which the game reads (id: rac2-ntsc)
//! <data home>/texture_dumps/<id>/       the textures the game saved while dumping, sorted into
//!     Interface/  Shared/  Planets/<planet>/    (port/renderer/texture_pack.cpp)
//! <data home>/texture_dumps/<id>.dumping   there while "Save the game's textures" is on
//! ```
//!
//! `<data>` is the game's folder in the install folder (install.rs `data_dir`), the `--data` Play
//! passes. A pack travels as a .zip of its folders with a manifest ([`MANIFEST`]) naming the game.
//! Importing checks the whole file before anything is written; exporting leaves out the pictures
//! that are still the disc's own (byte for byte the dump's), since those are not the player's to
//! share. The `texture_pack` setting in the version's settings file switches the pack on or off.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use crate::differences::{read_switch, settings_file, with_value, write_settings};

/// The pack's manifest, at the top of its .zip.
pub const MANIFEST: &str = "openrac-texture-pack.json";
/// The port's setting that reads the pack (port/game/common/settings.h); on unless it says 0.
pub const SWITCH: &str = "texture_pack";
/// Launcher-only switch: prepare/reuse the persistent Ultra4K pack before Play.
pub const ULTRA_SWITCH: &str = "offline_ultra_textures";
/// The environment variable that makes the game save its textures (port/common/env.cpp).
pub const DUMP_ENV: &str = "OPENRAC_DUMP_TEXTURES";

/// The most files and bytes a pack's .zip may unpack to.
const MAX_FILES: usize = 200_000;
const MAX_BYTES: u64 = 16 * 1024 * 1024 * 1024;
/// The largest single picture: a 4096 x 4096 RGBA PNG stored without compression is 64 MB.
const MAX_PICTURE: u64 = 128 * 1024 * 1024;
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// A folder of a pack or a dump and how many textures it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(rename = "TextureFolder")]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    /// Relative to the pack or dump, with `/`: `Interface`, `Shared`, `Planets/Oozla`; empty for
    /// the pictures at the top.
    pub path: String,
    /// The folder's name as the page shows it: `Interface`, `Oozla`.
    pub label: String,
    pub count: u32,
}

/// The Texture packs section of a game's Mods tab.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(rename = "TexturePackView")]
#[serde(rename_all = "camelCase")]
pub struct View {
    /// The port's id of the version: `rac2-ntsc`.
    pub id: String,
    /// The `texture_pack` setting.
    pub enabled: bool,
    /// Whether the next Play saves the textures.
    pub dumping: bool,
    /// Whether the launcher prepares/reuses the persistent Ultra4K pack before Play.
    pub ultra_enabled: bool,
    /// Whether a completed Ultra4K pack is present for the current sources.
    pub ultra_ready: bool,
    pub ultra_textures: u32,
    pub ultra_bytes: u64,
    pub pack_count: u32,
    /// Of those, the animated ones (a folder of frames or a GIF named by the texture).
    pub pack_animated: u32,
    pub pack: Vec<Folder>,
    pub dump_count: u32,
    pub dump: Vec<Folder>,
}

/// What a pack's .zip holds, before importing it.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(rename = "TexturePackPreview")]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub textures: u32,
    /// Of those, the animated ones.
    pub animated: u32,
    pub folders: Vec<Folder>,
    /// The pack's name, from its manifest.
    pub name: Option<String>,
    /// The textures the player's pack has now, and how many of them the import would replace.
    pub current: u32,
    pub replacing: u32,
    /// Files that are not pictures of textures (a readme), left out.
    pub skipped: u32,
}

/// An exported pack: where it went (empty when the player backed out), what it holds.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(rename = "ExportedTexturePack")]
#[serde(rename_all = "camelCase")]
pub struct Exported {
    pub saved: String,
    pub textures: u32,
    /// The pictures still the disc's own, left out.
    pub left_out: u32,
}

/// An import done: the pictures added and, on a replace, the folder the old pack was kept in
/// (its name only).
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(rename = "ImportedTexturePack")]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    pub textures: u32,
    pub kept: Option<String>,
}

/// The manifest's contents.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub format: u32,
    /// The port's id of the version the pack is for.
    #[serde(default)]
    pub game: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub textures: u32,
    #[serde(default, rename = "madeWith", skip_serializing_if = "Option::is_none")]
    pub made_with: Option<String>,
}

/// The port's id of a version (port/game/<id>/hostgen.json): `rac2` `ntsc` is `rac2-ntsc`.
pub fn id(game: &str, version: &str) -> Result<String, String> {
    let plain = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !plain(game) || !plain(version) {
        return Err(format!("{game}/{version} is not a game version"));
    }
    Ok(format!("{game}-{version}"))
}

/// The version's pack folder: `<data>/texture_packs/<id>`.
pub fn pack_dir(data: &Path, id: &str) -> PathBuf {
    data.join("texture_packs").join(id)
}

/// The version's dump folder: `<data home>/texture_dumps/<id>`.
pub fn dump_dir(data_home: &Path, id: &str) -> PathBuf {
    data_home.join("texture_dumps").join(id)
}

fn dump_flag(data_home: &Path, id: &str) -> PathBuf {
    data_home.join("texture_dumps").join(format!("{id}.dumping"))
}

/// A game version's name for a message: `Going Commando (NTSC-U)`, else its id.
pub fn title(id: &str) -> String {
    match id {
        "rac1-pal" => "Ratchet & Clank (PAL)".into(),
        "rac1-ntsc" => "Ratchet & Clank (NTSC-U)".into(),
        "rac1-pal1" => "Ratchet & Clank (PAL v1.00)".into(),
        "rac1-jp" => "Ratchet & Clank (Japan)".into(),
        "rac2-ntsc" => "Going Commando (NTSC-U)".into(),
        "rac2-pal" => "Going Commando (PAL)".into(),
        "rac2-ntsc2" => "Going Commando (NTSC-U v2.00)".into(),
        "rac3-ntsc" => "Up Your Arsenal (NTSC-U)".into(),
        "rac3-pal" => "Up Your Arsenal (PAL)".into(),
        "rac4-ntsc" => "Deadlocked (NTSC-U)".into(),
        other => other.into(),
    }
}

/// Whether a name is a texture's hash: 16 hex digits, either case.
fn is_hash(name: &str) -> bool {
    name.len() == 16 && name.chars().all(|c| c.is_ascii_hexdigit())
}

/// A texture's hash from a file name with the extension `ext` (`0123456789abcdef.png`, either
/// case), lowercase.
fn hash_named(file_name: &str, ext: &str) -> Option<String> {
    let (stem, e) = file_name.rsplit_once('.')?;
    (e.eq_ignore_ascii_case(ext) && is_hash(stem)).then(|| stem.to_ascii_lowercase())
}

/// A texture's hash from a picture's file name (`0123456789abcdef.png`, either case), lowercase.
pub fn texture_name(file_name: &str) -> Option<String> {
    hash_named(file_name, "png")
}

fn has_ext(file_name: &str, ext: &str) -> bool {
    file_name.rsplit_once('.').is_some_and(|(_, e)| e.eq_ignore_ascii_case(ext))
}

/// One texture of a pack: a picture named by its hash, an animated GIF named so, or a folder named
/// so that holds an animation's frames (`0001.png`, ... and an optional `animation.json`). A
/// picture named by a hash is always a texture of its own, wherever it lies, so a folder named by a
/// hash that holds only such pictures (the dump's) is an ordinary folder.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Item {
    hash: String,
    /// The picture, or the animation's folder, relative to the pack.
    rel: PathBuf,
    animated: bool,
}

/// Whether a file belongs to an animation's folder: a frame (a PNG not named by a hash) or its
/// settings (a .json).
fn frame_file(file_name: &str) -> bool {
    (has_ext(file_name, "png") && texture_name(file_name).is_none()) || has_ext(file_name, "json")
}

/// Every texture under a folder.
fn items_under(root: &Path) -> Vec<Item> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(root.join(&rel)) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                let frames = is_hash(&name)
                    && std::fs::read_dir(entry.path()).is_ok_and(|files| {
                        files.flatten().any(|f| {
                            let n = f.file_name().to_string_lossy().into_owned();
                            has_ext(&n, "png") && texture_name(&n).is_none()
                        })
                    });
                if frames {
                    out.push(Item { hash: name.to_ascii_lowercase(), rel: rel.join(&name), animated: true });
                }
                // Its pictures named by a hash are textures all the same.
                stack.push(rel.join(&name));
            } else if kind.is_file() {
                if let Some(hash) = texture_name(&name) {
                    out.push(Item { hash, rel: rel.join(&name), animated: false });
                } else if let Some(hash) = hash_named(&name, "gif") {
                    out.push(Item { hash, rel: rel.join(&name), animated: true });
                }
            }
        }
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    out
}

/// The player's hand-made/imported pack items. The generated Ultra4K cache is local build
/// output, not part of the editable/shareable pack.
fn user_items(root: &Path) -> Vec<Item> {
    items_under(root).into_iter().filter(|item| !item.rel.starts_with(ULTRA_FOLDER)).collect()
}

/// The folder a texture counts under: `Planets/<planet>` for a planet's, else its top folder
/// (empty at the top).
fn group_of(rel: &Path) -> String {
    let parts: Vec<String> = rel.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    match parts.as_slice() {
        [first, second, _, ..] if first == "Planets" => format!("Planets/{second}"),
        [first, _, ..] => first.clone(),
        _ => String::new(),
    }
}

/// The dump's and a pack's usual folders, in the order the page lists them; a planet's
/// (`Planets/<planet>`) come after them, any other folder next, the pictures at the top last.
/// A new kind of folder (the dump's animations) is one more name here.
const FOLDER_ORDER: &[&str] = &["Interface", "Shared", "Animated"];

/// Counts by folder, in the page's order.
fn count_folders(items: &[Item]) -> Vec<Folder> {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for item in items {
        *counts.entry(group_of(&item.rel)).or_default() += 1;
    }
    let rank = |path: &str| match FOLDER_ORDER.iter().position(|f| *f == path) {
        Some(i) => i,
        None if path.starts_with("Planets/") => FOLDER_ORDER.len(),
        None if path.is_empty() => FOLDER_ORDER.len() + 2,
        None => FOLDER_ORDER.len() + 1,
    };
    let mut folders: Vec<Folder> = counts
        .into_iter()
        .map(|(path, count)| {
            let label = match path.rsplit_once('/') {
                Some((_, last)) => last.to_string(),
                None if path.is_empty() => "Other pictures".to_string(),
                None => path.clone(),
            };
            Folder { path, label, count }
        })
        .collect();
    folders.sort_by(|a, b| rank(&a.path).cmp(&rank(&b.path)).then_with(|| a.label.cmp(&b.label)));
    folders
}

/// The section's state for a version.
pub fn view(data_home: &Path, data: &Path, game: &str, version: &str) -> Result<View, String> {
    let id = id(game, version)?;
    let settings = std::fs::read_to_string(settings_file(data_home, game, version)).unwrap_or_default();
    let pack_root = pack_dir(data, &id);
    let pack = user_items(&pack_root);
    let dump = items_under(&dump_dir(data_home, &id));
    let ultra = read_ultra_stamp(&pack_root);
    Ok(View {
        enabled: read_switch(&settings, SWITCH).unwrap_or(true),
        dumping: dump_flag(data_home, &id).is_file(),
        ultra_enabled: read_switch(&settings, ULTRA_SWITCH).unwrap_or(true),
        ultra_ready: ultra.is_some(),
        ultra_textures: ultra.as_ref().map_or(0, |s| s.textures),
        ultra_bytes: ultra.as_ref().map_or(0, |s| s.bytes),
        pack_count: pack.len() as u32,
        pack_animated: pack.iter().filter(|i| i.animated).count() as u32,
        pack: count_folders(&pack),
        dump_count: dump.len() as u32,
        dump: count_folders(&dump),
        id,
    })
}

/// Switches the pack on or off in the version's settings file.
pub fn set_enabled(data_home: &Path, game: &str, version: &str, on: bool) -> Result<(), String> {
    id(game, version)?;
    let file = settings_file(data_home, game, version);
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    write_settings(&file, &with_value(&text, SWITCH, if on { "1" } else { "0" }))
}

/// Enables/disables offline Ultra4K preparation without removing cached or imported pictures.
pub fn set_ultra_enabled(data_home: &Path, _data: &Path, game: &str, version: &str, on: bool) -> Result<(), String> {
    id(game, version)?;
    let file = settings_file(data_home, game, version);
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    write_settings(&file, &with_value(&text, ULTRA_SWITCH, if on { "1" } else { "0" }))
}

/// Explicit cache deletion, separate from disabling its use. The native lock prevents deletion
/// while another preparation owns this cache; imported pictures are outside Ultra4K.
pub fn delete_ultra(data: &Path, game: &str, version: &str) -> Result<(), String> {
    let pack = pack_dir(data, &id(game, version)?);
    if !pack.is_dir() {
        return Ok(());
    }
    let _lock = acquire_ultra_lock(&pack)?;
    let ultra = pack.join(ULTRA_FOLDER);
    if ultra.exists() {
        std::fs::remove_dir_all(&ultra).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Switches saving the textures on or off for the next Play.
pub fn set_dumping(data_home: &Path, game: &str, version: &str, on: bool) -> Result<(), String> {
    let id = id(game, version)?;
    let flag = dump_flag(data_home, &id);
    if on {
        std::fs::create_dir_all(dump_dir(data_home, &id)).map_err(|e| e.to_string())?;
        std::fs::write(&flag, b"").map_err(|e| format!("{}: {e}", flag.display()))
    } else {
        match std::fs::remove_file(&flag) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("{}: {e}", flag.display())),
            _ => Ok(()),
        }
    }
}

/// What Play adds while saving the textures is on: the dump folder, for the game.
pub fn play_env(data_home: &Path, game: &str, version: &str) -> Option<(String, String)> {
    let id = id(game, version).ok()?;
    dump_flag(data_home, &id).is_file().then(|| (DUMP_ENV.to_string(), dump_dir(data_home, &id).display().to_string()))
}

/// The launcher-built, persistent high-resolution pack. It is deliberately a subfolder of the
/// player's pack: hand-made/imported textures outside it win, and a rebuild never deletes them.
pub const ULTRA_FOLDER: &str = "Ultra4K";
const ULTRA_STAMP: &str = "openrac-ultra4k.json";
const ULTRA_ALGORITHM: u32 = 2;
const ULTRA_SCALE: u32 = 8;
const ULTRA_MAX_EDGE: u32 = 4096;

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(rename = "UltraTextureBuild")]
#[serde(rename_all = "camelCase")]
pub struct UltraTextureBuild {
    pub enabled: bool,
    pub cached: bool,
    pub source_files: u32,
    pub textures: u32,
    pub skipped_custom: u32,
    pub bytes: u64,
    pub max_edge: u32,
    pub scale: u32,
}

impl UltraTextureBuild {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            cached: true,
            source_files: 0,
            textures: 0,
            skipped_custom: 0,
            bytes: 0,
            max_edge: ULTRA_MAX_EDGE,
            scale: ULTRA_SCALE,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UltraStamp {
    algorithm: u32,
    source_fingerprint: String,
    source_files: u32,
    textures: u32,
    skipped_custom: u32,
    bytes: u64,
    max_edge: u32,
    scale: u32,
}

fn read_ultra_stamp(pack: &Path) -> Option<UltraStamp> {
    let ultra = pack.join(ULTRA_FOLDER);
    let text = std::fs::read_to_string(ultra.join(ULTRA_STAMP)).ok()?;
    let stamp: UltraStamp = serde_json::from_str(&text).ok()?;
    if stamp.algorithm != ULTRA_ALGORITHM || stamp.max_edge != ULTRA_MAX_EDGE || stamp.scale != ULTRA_SCALE {
        return None;
    }
    let pictures = png_files(&ultra);
    if pictures.len() as u32 != stamp.textures {
        return None;
    }
    let bytes: u64 = pictures.iter().filter_map(|path| std::fs::metadata(path).ok().map(|m| m.len())).sum();
    (bytes == stamp.bytes).then_some(stamp)
}

#[derive(Debug, Clone)]
struct UltraSource {
    path: PathBuf,
    width: u32,
    height: u32,
}

fn png_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn source_fingerprint(port: &Path, dump: &Path, pack: &Path) -> (String, u32) {
    let mut sources: Vec<(String, PathBuf)> = Vec::new();
    for path in png_files(port) {
        sources.push(("port".into(), path));
    }
    for path in png_files(dump) {
        sources.push(("dump".into(), path));
    }
    // A hand-made/imported texture changes which generated hashes must be omitted, so it is part
    // of the fingerprint too. Never fingerprint our own generated folder.
    for item in items_under(pack) {
        if !item.rel.starts_with(ULTRA_FOLDER) {
            sources.push(("custom".into(), pack.join(item.rel)));
        }
    }
    sources.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut digest = Sha256::new();
    digest.update(ULTRA_ALGORITHM.to_le_bytes());
    digest.update(ULTRA_SCALE.to_le_bytes());
    digest.update(ULTRA_MAX_EDGE.to_le_bytes());
    for (kind, path) in &sources {
        digest.update(kind.as_bytes());
        digest.update([0]);
        digest.update(path.to_string_lossy().as_bytes());
        digest.update([0]);
        if let Ok(meta) = std::fs::metadata(path) {
            digest.update(meta.len().to_le_bytes());
            if let Ok(time) =
                meta.modified().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).map_err(std::io::Error::other))
            {
                digest.update(time.as_secs().to_le_bytes());
                digest.update(time.subsec_nanos().to_le_bytes());
            }
        }
    }
    (digest.finalize().iter().map(|b| format!("{b:02x}")).collect(), sources.len() as u32)
}

fn png_rgba(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("{}: {e}", path.display()))?;
    let mut raw =
        vec![0; reader.output_buffer_size().ok_or_else(|| format!("{}: decoded PNG is too large", path.display()))?];
    let info = reader.next_frame(&mut raw).map_err(|e| format!("{}: {e}", path.display()))?;
    raw.truncate(info.buffer_size());
    let pixels = match info.color_type {
        png::ColorType::Rgba => raw,
        png::ColorType::Rgb => raw.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => {
            raw.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect()
        }
        png::ColorType::Grayscale => raw.iter().flat_map(|v| [*v, *v, *v, 255]).collect(),
        png::ColorType::Indexed => return Err(format!("{} stayed indexed after PNG expansion", path.display())),
    };
    Ok((info.width, info.height, pixels))
}

fn image_hash(width: u32, height: u32, pixels: &[u8]) -> u64 {
    let mut h = 1469598103934665603u64;
    let mut mix = |byte: u8| {
        h ^= u64::from(byte);
        h = h.wrapping_mul(1099511628211u64);
    };
    for side in [width, height] {
        for byte in side.to_le_bytes() {
            mix(byte);
        }
    }
    for &byte in pixels {
        mix(byte);
    }
    h
}

fn png_dimensions(path: &Path) -> Result<(u32, u32), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let reader = decoder.read_info().map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((reader.info().width, reader.info().height))
}

fn ultra_dimensions(width: u32, height: u32) -> (u32, u32) {
    let longest = width.max(height).max(1);
    let scale = (ULTRA_SCALE as f64).min(ULTRA_MAX_EDGE as f64 / longest as f64).max(1.0);
    (((width as f64 * scale).round() as u32).max(1), ((height as f64 * scale).round() as u32).max(1))
}

#[cfg(any(not(target_os = "macos"), test))]
fn write_rgba_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| format!("{}: {e}", path.display()))?;
    writer.write_image_data(pixels).map_err(|e| format!("{}: {e}", path.display()))
}

/// Portable fallback used when the OS has no native offline resampler. Interpolation happens in
/// premultiplied-alpha space so transparent texture edges do not grow dark/coloured halos.
#[cfg(any(not(target_os = "macos"), test))]
fn resize_rgba(width: u32, height: u32, pixels: &[u8], out_width: u32, out_height: u32) -> Vec<u8> {
    if width == out_width && height == out_height {
        return pixels.to_vec();
    }
    let mut out = vec![0u8; out_width as usize * out_height as usize * 4];
    let sx = width as f32 / out_width as f32;
    let sy = height as f32 / out_height as f32;
    let sample = |x: u32, y: u32| {
        let at = ((y * width + x) * 4) as usize;
        let a = pixels[at + 3] as f32 / 255.0;
        [pixels[at] as f32 * a, pixels[at + 1] as f32 * a, pixels[at + 2] as f32 * a, pixels[at + 3] as f32]
    };
    for oy in 0..out_height {
        let fy = ((oy as f32 + 0.5) * sy - 0.5).clamp(0.0, height.saturating_sub(1) as f32);
        let y0 = fy.floor() as u32;
        let y1 = (y0 + 1).min(height - 1);
        let ty = fy - y0 as f32;
        for ox in 0..out_width {
            let fx = ((ox as f32 + 0.5) * sx - 0.5).clamp(0.0, width.saturating_sub(1) as f32);
            let x0 = fx.floor() as u32;
            let x1 = (x0 + 1).min(width - 1);
            let tx = fx - x0 as f32;
            let p00 = sample(x0, y0);
            let p10 = sample(x1, y0);
            let p01 = sample(x0, y1);
            let p11 = sample(x1, y1);
            let mut mixed = [0.0f32; 4];
            for c in 0..4 {
                let top = p00[c] + (p10[c] - p00[c]) * tx;
                let bottom = p01[c] + (p11[c] - p01[c]) * tx;
                mixed[c] = top + (bottom - top) * ty;
            }
            let at = ((oy * out_width + ox) * 4) as usize;
            let alpha = mixed[3].clamp(0.0, 255.0);
            if alpha > 0.001 {
                let unpremultiply = 255.0 / alpha;
                out[at] = (mixed[0] * unpremultiply).round().clamp(0.0, 255.0) as u8;
                out[at + 1] = (mixed[1] * unpremultiply).round().clamp(0.0, 255.0) as u8;
                out[at + 2] = (mixed[2] * unpremultiply).round().clamp(0.0, 255.0) as u8;
            }
            out[at + 3] = alpha.round() as u8;
        }
    }
    out
}

#[cfg(any(not(target_os = "macos"), test))]
fn portable_resample(input: &Path, output: &Path, width: u32, height: u32) -> Result<(), String> {
    let (source_width, source_height, pixels) = png_rgba(input)?;
    let resized = resize_rgba(source_width, source_height, &pixels, width, height);
    write_rgba_png(output, width, height, &resized)
}

fn folder_bytes(root: &Path) -> u64 {
    png_files(root).into_iter().filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len())).sum()
}

struct UltraBuildLock {
    path: PathBuf,
}

impl Drop for UltraBuildLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// One Ultra build per game data folder, even when two launcher windows/agents point at it. A
/// crashed process can leave a lock behind; after fifteen minutes it is considered stale. Normal
/// builds refresh the lock's mtime before each expensive phase, so a live long build is not stolen.
fn acquire_ultra_lock(pack: &Path) -> Result<UltraBuildLock, String> {
    use std::io::ErrorKind;
    use std::time::{Duration, SystemTime};

    let path = pack.join(format!(".{ULTRA_FOLDER}.lock"));
    for _ in 0..480 {
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                use std::io::Write as _;
                let _ = writeln!(file, "{}", std::process::id());
                return Ok(UltraBuildLock { path });
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                let stale = std::fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| SystemTime::now().duration_since(t).ok())
                    .is_some_and(|age| age > Duration::from_secs(15 * 60));
                if stale {
                    let _ = std::fs::remove_file(&path);
                    continue;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(e) => return Err(format!("cannot lock Ultra texture build: {e}")),
        }
    }
    Err("another launcher is still preparing Ultra textures".into())
}

fn touch_ultra_lock(lock: &UltraBuildLock) {
    // Rewriting the tiny file refreshes its modification time without changing lock ownership.
    let _ = std::fs::write(&lock.path, format!("{}\n", std::process::id()));
}

/// Builds all static exported world textures, plus every texture already observed by the runtime
/// dumper (notably HUD/interface), into a persistent 8x pack capped at a 4096-pixel edge. The
/// expensive resampling happens here, before Play. Existing user/imported pack pictures are never
/// replaced: their hashes are omitted from Ultra4K.
pub fn prepare_ultra4k(data_home: &Path, data: &Path, game: &str, version: &str) -> Result<UltraTextureBuild, String> {
    #[cfg(target_os = "macos")]
    use std::process::{Command, Stdio};

    let id = id(game, version)?;
    let settings = std::fs::read_to_string(settings_file(data_home, game, version)).unwrap_or_default();
    if !read_switch(&settings, ULTRA_SWITCH).unwrap_or(true) {
        return Ok(UltraTextureBuild::disabled());
    }
    let port = data.join("port").join(version);
    if !port.is_dir() {
        return Err(format!("{} has no exported level textures yet", port.display()));
    }
    let dump = dump_dir(data_home, &id);
    let pack = pack_dir(data, &id);
    std::fs::create_dir_all(&pack).map_err(|e| format!("{}: {e}", pack.display()))?;
    let lock = acquire_ultra_lock(&pack)?;
    let ultra = pack.join(ULTRA_FOLDER);
    let (fingerprint, source_files) = source_fingerprint(&port, &dump, &pack);
    let previous = read_ultra_stamp(&pack);
    if let Some(stamp) = previous.as_ref() {
        if stamp.source_fingerprint == fingerprint {
            return Ok(UltraTextureBuild {
                enabled: true,
                cached: true,
                source_files: stamp.source_files,
                textures: stamp.textures,
                skipped_custom: stamp.skipped_custom,
                bytes: stamp.bytes,
                max_edge: stamp.max_edge,
                scale: stamp.scale,
            });
        }
    }

    let custom: HashSet<String> = items_under(&pack)
        .into_iter()
        .filter(|item| !item.rel.starts_with(ULTRA_FOLDER))
        .map(|item| item.hash)
        .collect();
    let mut sources: BTreeMap<String, UltraSource> = BTreeMap::new();

    // Dumped textures already have the exact pack hash in their file name. Prefer these so the
    // Interface/Shared pictures that do not exist in the static level export are included too.
    for path in png_files(&dump) {
        let Some(name) = path.file_name().and_then(|n| n.to_str()).and_then(texture_name) else { continue };
        if custom.contains(&name) || sources.contains_key(&name) {
            continue;
        }
        let (width, height) = png_dimensions(&path)?;
        sources.insert(name, UltraSource { path, width, height });
    }

    let mut skipped_custom = 0u32;
    touch_ultra_lock(&lock);
    // Static exported level textures are palette PNGs. Decode them exactly as the renderer does,
    // then reproduce texture_pack::image_hash so the generated PNG replaces the right texture.
    for path in png_files(&port) {
        let (width, height, rgba) = png_rgba(&path)?;
        let hash = format!("{:016x}", image_hash(width, height, &rgba));
        if custom.contains(&hash) {
            skipped_custom += 1;
            continue;
        }
        sources.entry(hash).or_insert(UltraSource { path, width, height });
    }

    let staging = pack.join(format!(".{ULTRA_FOLDER}-building-{}", std::process::id()));
    let input = staging.join("input");
    let output = staging.join("output");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&input).map_err(|e| format!("{}: {e}", input.display()))?;
    std::fs::create_dir_all(&output).map_err(|e| format!("{}: {e}", output.display()))?;

    let build = (|| -> Result<(), String> {
        let mut groups: BTreeMap<(u32, u32), Vec<PathBuf>> = BTreeMap::new();
        for (hash, source) in &sources {
            let generated = output.join(format!("{hash}.png"));
            // A source hash identifies the exact original dimensions and texels. When only the
            // source set changed, a generated picture with that same hash and the same algorithm
            // is still valid: link/copy it into the new staging pack instead of resampling it.
            if previous.is_some() {
                let old = ultra.join(format!("{hash}.png"));
                if old.is_file() {
                    if std::fs::hard_link(&old, &generated).is_err() {
                        std::fs::copy(&old, &generated).map_err(|e| format!("{}: {e}", generated.display()))?;
                    }
                    continue;
                }
            }
            let staged = input.join(format!("{hash}.png"));
            if std::fs::hard_link(&source.path, &staged).is_err() {
                std::fs::copy(&source.path, &staged).map_err(|e| format!("{}: {e}", staged.display()))?;
            }
            groups.entry(ultra_dimensions(source.width, source.height)).or_default().push(staged);
        }

        for ((width, height), files) in groups {
            touch_ultra_lock(&lock);
            #[cfg(target_os = "macos")]
            {
                // CoreGraphics is considerably faster than the portable CPU fallback on Apple
                // hardware. Batching also avoids thousands of process launches.
                for batch in files.chunks(128) {
                    let status = Command::new("/usr/bin/sips")
                        .arg("-z")
                        .arg(height.to_string())
                        .arg(width.to_string())
                        .arg("--out")
                        .arg(&output)
                        .args(batch)
                        .stdout(Stdio::null())
                        .stderr(Stdio::piped())
                        .status()
                        .map_err(|e| format!("cannot start macOS texture resampler: {e}"))?;
                    if !status.success() {
                        return Err(format!("macOS texture resampler stopped with {status}"));
                    }
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                // Windows/Linux builds have no dependency on a platform image utility: decode,
                // resize and encode in-process so the same launcher bundle works everywhere.
                for file in &files {
                    let name = file.file_name().ok_or_else(|| format!("{} has no file name", file.display()))?;
                    portable_resample(file, &output.join(name), width, height)?;
                    touch_ultra_lock(&lock);
                }
            }
        }
        Ok(())
    })();
    if let Err(e) = build {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }

    let bytes = folder_bytes(&output);
    let stamp = UltraStamp {
        algorithm: ULTRA_ALGORITHM,
        source_fingerprint: fingerprint,
        source_files,
        textures: sources.len() as u32,
        skipped_custom,
        bytes,
        max_edge: ULTRA_MAX_EDGE,
        scale: ULTRA_SCALE,
    };
    std::fs::write(output.join(ULTRA_STAMP), serde_json::to_vec_pretty(&stamp).map_err(|e| e.to_string())?)
        .map_err(|e| format!("could not write Ultra texture stamp: {e}"))?;

    touch_ultra_lock(&lock);
    let previous_dir = pack.join(format!(".{ULTRA_FOLDER}-previous-{}", std::process::id()));
    let had_previous = ultra.is_dir();
    if had_previous {
        let _ = std::fs::remove_dir_all(&previous_dir);
        std::fs::rename(&ultra, &previous_dir).map_err(|e| format!("{}: {e}", ultra.display()))?;
    }
    if let Err(e) = std::fs::rename(&output, &ultra) {
        if had_previous {
            let _ = std::fs::rename(&previous_dir, &ultra);
        }
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!("{}: {e}", ultra.display()));
    }
    if had_previous {
        let _ = std::fs::remove_dir_all(&previous_dir);
    }
    let _ = std::fs::remove_dir_all(&staging);
    Ok(UltraTextureBuild {
        enabled: true,
        cached: false,
        source_files,
        textures: sources.len() as u32,
        skipped_custom,
        bytes,
        max_edge: ULTRA_MAX_EDGE,
        scale: ULTRA_SCALE,
    })
}

/// A folder of the pack or the dump to open, made when it is not there yet. `sub` is one of the
/// view's folder paths (empty: the whole pack or dump).
pub fn folder(
    data_home: &Path,
    data: &Path,
    game: &str,
    version: &str,
    dump: bool,
    sub: &str,
) -> Result<PathBuf, String> {
    let id = id(game, version)?;
    let root = if dump { dump_dir(data_home, &id) } else { pack_dir(data, &id) };
    if !sub.is_empty() && !crate::plugins::safe_relative(sub) {
        return Err(format!("{sub} is not a folder of the pack"));
    }
    let dir = if sub.is_empty() { root } else { root.join(sub) };
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

/// A file of a pack's .zip that is imported: its index in the archive and its path in the pack.
type ZipFile = (usize, PathBuf);

/// A pack's .zip, checked: its textures with their files, its manifest.
struct Scanned {
    items: Vec<(Item, Vec<ZipFile>)>,
    // Materials/<hash>/<map> files are tied to a colour texture but are not replacements themselves.
    materials: Vec<(String, ZipFile)>,
    manifest: Manifest,
    skipped: u32,
}

/// What a mod's .zip is told when it is read as a texture pack.
pub const NOT_A_PACK_BUT_A_MOD: &str = "This .zip is a mod, not a texture pack.";

/// The folders a pack keeps at its top; a single other top folder (the pack zipped with its own
/// folder) is taken off.
const PACK_FOLDERS: &[&str] = &["Interface", "Shared", "Planets", "Animated", "Materials"];

const MATERIAL_FILES: &[&str] = &[
    "normal.png",
    "roughness.png",
    "height.png",
    "ao.png",
    "emissive.png",
    "specular.png",
    "metallic.png",
    "cavity.png",
    "curvature.png",
    "edgewear.png",
    "material.json",
];

fn material_sidecar(rel: &str) -> Option<(String, PathBuf)> {
    let parts: Vec<&str> = rel.split('/').collect();
    if parts.len() != 3 || parts[0] != "Materials" || !is_hash(parts[1]) || !MATERIAL_FILES.contains(&parts[2]) {
        return None;
    }
    Some((parts[1].to_ascii_lowercase(), PathBuf::from(rel)))
}

fn open_zip(path: &Path) -> Result<zip::ZipArchive<std::fs::File>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    zip::ZipArchive::new(file).map_err(|_| "This file is not a .zip.".to_string())
}

fn scan(archive: &mut zip::ZipArchive<std::fs::File>, id: &str) -> Result<Scanned, String> {
    if archive.len() > MAX_FILES {
        return Err(format!("The .zip holds more than {MAX_FILES} files: it is not a texture pack."));
    }
    // Every name first: nothing outside the pack's folder, no links, sizes within reason.
    let mut names: Vec<(usize, String)> = Vec::new();
    let mut declared: u64 = 0;
    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i).map_err(|e| e.to_string())?;
        let name = entry.name().map_err(|e| e.to_string())?.replace('\\', "/");
        let unsafe_name = name.starts_with('/')
            || name.contains(':')
            || name.split('/').any(|p| p == "..")
            || entry.enclosed_name().is_none();
        if unsafe_name {
            return Err(format!("The .zip names a file outside its folder ({name}): it is not added."));
        }
        if entry.is_symlink() {
            return Err(format!("The .zip holds a link ({name}): a pack holds pictures only."));
        }
        declared = declared.saturating_add(entry.size());
        if declared > MAX_BYTES || entry.size() > MAX_PICTURE {
            return Err("The .zip is too large to be a texture pack.".into());
        }
        if !entry.is_dir() {
            names.push((i, name.trim_end_matches('/').to_string()));
        }
    }
    let first = |n: &str| n.split('/').next().unwrap_or_default().to_string();
    let tops: std::collections::BTreeSet<String> =
        names.iter().filter(|(_, n)| n.contains('/') && !n.starts_with("__MACOSX/")).map(|(_, n)| first(n)).collect();
    let loose = names.iter().any(|(_, n)| !n.contains('/'));
    let strip = match tops.iter().next() {
        Some(top) if tops.len() == 1 && !loose && !PACK_FOLDERS.contains(&top.as_str()) && !is_hash(top) => {
            Some(format!("{top}/"))
        }
        _ => None,
    };
    let mut items: BTreeMap<String, (Item, Vec<ZipFile>)> = BTreeMap::new();
    let mut materials: Vec<(String, ZipFile)> = Vec::new();
    let mut manifest = None;
    let mut skipped = 0;
    let mut bad: Vec<String> = Vec::new();
    for (i, name) in names {
        let rel = strip.as_deref().and_then(|s| name.strip_prefix(s)).unwrap_or(&name).to_string();
        let (folder, file_name) = rel.rsplit_once('/').unwrap_or(("", &rel));
        if rel.starts_with("__MACOSX/") || file_name.starts_with('.') {
            skipped += 1;
            continue;
        }
        // A mod's manifest anywhere: the .zip is a mod (which may carry pictures of its own), not
        // a texture pack. The launcher's Add a mod then installs it as one.
        if file_name == crate::plugins::MANIFEST {
            return Err(NOT_A_PACK_BUT_A_MOD.into());
        }
        if rel == MANIFEST {
            let mut text = String::new();
            archive
                .by_index(i)
                .map_err(|e| e.to_string())?
                .take(1024 * 1024)
                .read_to_string(&mut text)
                .map_err(|_| "The pack's manifest cannot be read.".to_string())?;
            manifest = Some(
                serde_json::from_str::<Manifest>(&text)
                    .map_err(|_| "The pack's manifest cannot be read.".to_string())?,
            );
            continue;
        }
        if let Some((hash, path)) = material_sidecar(&rel) {
            materials.push((hash, (i, path)));
            continue;
        }
        let parent = folder.rsplit('/').next().unwrap_or_default();
        let (item, file) = if let Some(hash) = texture_name(file_name) {
            let path = Path::new(folder).join(format!("{hash}.png"));
            (Item { hash, rel: path.clone(), animated: false }, path)
        } else if let Some(hash) = hash_named(file_name, "gif") {
            let path = Path::new(folder).join(format!("{hash}.gif"));
            (Item { hash, rel: path.clone(), animated: true }, path)
        } else if is_hash(parent) && frame_file(file_name) {
            // An animation's frame or its settings, in the folder named by its texture.
            let hash = parent.to_ascii_lowercase();
            let above = folder.rsplit_once('/').map(|(a, _)| a).unwrap_or("");
            let dir = Path::new(above).join(&hash);
            (Item { hash, rel: dir.clone(), animated: true }, dir.join(file_name))
        } else if has_ext(file_name, "png") {
            bad.push(rel.clone());
            continue;
        } else {
            skipped += 1;
            continue;
        };
        match items.get_mut(&item.hash) {
            Some((known, files)) if known.rel == item.rel && known.animated && !has_ext(file_name, "gif") => {
                files.push((i, file));
            }
            Some((known, _)) => {
                return Err(format!(
                    "The pack has two pictures for the same texture ({} and {}).",
                    known.rel.display(),
                    item.rel.display()
                ));
            }
            None => {
                items.insert(item.hash.clone(), (item, vec![(i, file)]));
            }
        }
    }
    let manifest = manifest.unwrap_or_default();
    if let Some(game) = manifest.game.as_deref() {
        if game != id {
            return Err(format!(
                "This pack is for {}, not {}. Add it on that game's Mods tab instead.",
                title(game),
                title(id)
            ));
        }
    }
    if !bad.is_empty() {
        let examples: Vec<&str> = bad.iter().take(3).map(String::as_str).collect();
        return Err(format!(
            "{} picture(s) in the pack are not named after a texture (for example {}). A pack's pictures keep the names the game saved them under.",
            bad.len(),
            examples.join(", ")
        ));
    }
    for (item, files) in items.values() {
        let frames = files.iter().any(|(_, f)| f.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")));
        if item.animated && item.rel.extension().is_none() && !frames {
            return Err(format!("The animation folder {} has no frames.", item.rel.display()));
        }
    }
    for (hash, (_, path)) in &materials {
        if !items.contains_key(hash) {
            return Err(format!("Material sidecar {} has no colour texture {} in this pack.", path.display(), hash));
        }
    }
    if items.is_empty() {
        return Err("This .zip holds no texture pictures.".into());
    }
    Ok(Scanned { items: items.into_values().collect(), materials, manifest, skipped })
}

/// What a pack's .zip holds, refused when it is another game's or not a pack.
pub fn preview_import(data: &Path, game: &str, version: &str, source: &Path) -> Result<Preview, String> {
    let id = id(game, version)?;
    let scanned = scan(&mut open_zip(source)?, &id)?;
    let current: std::collections::HashSet<String> =
        user_items(&pack_dir(data, &id)).into_iter().map(|i| i.hash).collect();
    let items: Vec<Item> = scanned.items.iter().map(|(i, _)| i.clone()).collect();
    Ok(Preview {
        textures: items.len() as u32,
        animated: items.iter().filter(|i| i.animated).count() as u32,
        folders: count_folders(&items),
        name: scanned.manifest.name,
        current: current.len() as u32,
        replacing: items.iter().filter(|i| current.contains(&i.hash)).count() as u32,
        skipped: scanned.skipped,
    })
}

/// How an import treats the pack there already.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ts_rs::TS)]
#[ts(rename = "TexturePackImportMode")]
#[serde(rename_all = "lowercase")]
pub enum ImportMode {
    /// The pack there is kept beside it, renamed, and the imported one takes its place.
    Replace,
    /// The imported textures are added; one for a texture the pack has replaces it.
    Merge,
}

/// Checks a file's bytes are what its name says: a PNG, a GIF, or JSON.
fn check_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ok = if has_ext(&name, "png") {
        bytes.starts_with(PNG_MAGIC)
    } else if has_ext(&name, "gif") {
        bytes.starts_with(b"GIF8")
    } else {
        serde_json::from_slice::<serde_json::Value>(bytes).is_ok()
    };
    if ok {
        Ok(())
    } else if has_ext(&name, "json") {
        Err(format!("{} cannot be read.", path.display()))
    } else {
        Err(format!("{} is not a {} picture.", path.display(), if has_ext(&name, "gif") { "GIF" } else { "PNG" }))
    }
}

/// Imports a pack's .zip into the version's pack folder. Returns the textures imported and, on a
/// replace, the folder the old pack was kept in.
pub fn import(
    data: &Path,
    game: &str,
    version: &str,
    source: &Path,
    mode: ImportMode,
) -> Result<(u32, Option<PathBuf>), String> {
    let id = id(game, version)?;
    let mut archive = open_zip(source)?;
    let scanned = scan(&mut archive, &id)?;
    let pack = pack_dir(data, &id);
    let root = pack.parent().map(Path::to_path_buf).unwrap_or_default();
    // Unpacked beside the pack first, so a damaged picture halfway leaves the pack as it was.
    let staging = root.join(format!(".{id}.importing"));
    let _ = std::fs::remove_dir_all(&staging);
    let unpack = |archive: &mut zip::ZipArchive<std::fs::File>| -> Result<(), String> {
        for (i, rel) in
            scanned.items.iter().flat_map(|(_, files)| files).chain(scanned.materials.iter().map(|(_, file)| file))
        {
            let mut entry = archive.by_index(*i).map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            (&mut entry)
                .take(MAX_PICTURE + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| format!("{}: {e}", rel.display()))?;
            check_bytes(rel, &bytes)?;
            let target = staging.join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            std::fs::write(&target, bytes).map_err(|e| format!("{}: {e}", target.display()))?;
        }
        Ok(())
    };
    if let Err(e) = unpack(&mut archive) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    let count = scanned.items.len() as u32;
    let result = match mode {
        ImportMode::Replace => replace_with(&pack, &staging, &id),
        ImportMode::Merge => merge_into(&pack, &staging, &scanned.items, &scanned.materials).map(|()| None),
    };
    let _ = std::fs::remove_dir_all(&staging);
    result.map(|kept| (count, kept))
}

fn replace_with(pack: &Path, staging: &Path, id: &str) -> Result<Option<PathBuf>, String> {
    let mut kept = None;
    let parent = pack.parent().unwrap_or_else(|| Path::new("."));
    let held_ultra = parent.join(format!(".{id}.Ultra4K.keep-{}", std::process::id()));
    let ultra = pack.join(ULTRA_FOLDER);
    let had_ultra = ultra.is_dir();
    if had_ultra {
        let _ = std::fs::remove_dir_all(&held_ultra);
        std::fs::rename(&ultra, &held_ultra).map_err(|e| format!("{}: {e}", ultra.display()))?;
    }

    if pack.exists() {
        let stamp = crate::index_apply::stamp(crate::discord::now_sec());
        let old = pack.with_file_name(format!("{id} (before import {stamp})"));
        if let Err(e) = std::fs::rename(pack, &old) {
            if had_ultra {
                let _ = std::fs::rename(&held_ultra, &ultra);
            }
            return Err(format!("{}: {e}", pack.display()));
        }
        kept = Some(old);
    }

    if let Err(e) = std::fs::rename(staging, pack) {
        if let Some(old) = kept.as_ref() {
            let _ = std::fs::rename(old, pack);
        }
        if had_ultra {
            let _ = std::fs::create_dir_all(pack);
            let _ = std::fs::rename(&held_ultra, pack.join(ULTRA_FOLDER));
        }
        return Err(format!("{}: {e}", pack.display()));
    }
    if had_ultra {
        if let Err(e) = std::fs::rename(&held_ultra, pack.join(ULTRA_FOLDER)) {
            return Err(format!("offline Ultra cache could not be restored after import: {e}"));
        }
    }
    Ok(kept)
}

/// Removes a texture's picture or animation folder from the pack.
fn remove_item(pack: &Path, item: &Item) {
    let path = pack.join(&item.rel);
    if item.animated && item.rel.extension().is_none() {
        let _ = std::fs::remove_dir_all(path);
    } else {
        let _ = std::fs::remove_file(path);
    }
}

fn merge_into(
    pack: &Path,
    staging: &Path,
    items: &[(Item, Vec<ZipFile>)],
    materials: &[(String, ZipFile)],
) -> Result<(), String> {
    let current: HashMap<String, Item> = items_under(pack).into_iter().map(|i| (i.hash.clone(), i)).collect();
    for (item, files) in items {
        // One picture a texture: the pack's own gives way, and an animation is replaced whole.
        if let Some(old) = current.get(&item.hash) {
            remove_item(pack, old);
        }
        for (_, rel) in files {
            let target = pack.join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            std::fs::rename(staging.join(rel), &target).map_err(|e| format!("{}: {e}", target.display()))?;
        }
    }
    for (_, (_, rel)) in materials {
        let target = pack.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        // A newer material map replaces only the corresponding sidecar; other maps remain valid.
        let _ = std::fs::remove_file(&target);
        std::fs::rename(staging.join(rel), &target).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    Ok(())
}

/// The file name an exported pack is offered under.
pub fn export_name(id: &str) -> String {
    format!("{id}-texture-pack.zip")
}

/// An item's files on disk, relative to the pack: the picture, or the animation folder's frames
/// and settings.
fn item_files(pack: &Path, item: &Item) -> Vec<PathBuf> {
    let mut files = if !(item.animated && item.rel.extension().is_none()) {
        vec![item.rel.clone()]
    } else {
        let mut frames: Vec<PathBuf> = std::fs::read_dir(pack.join(&item.rel))
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| frame_file(n))
                    .map(|n| item.rel.join(n))
                    .collect()
            })
            .unwrap_or_default();
        frames.sort();
        frames
    };

    let material = Path::new("Materials").join(&item.hash);
    for name in MATERIAL_FILES {
        let rel = material.join(name);
        if pack.join(&rel).is_file() {
            files.push(rel);
        }
    }
    files
}

/// Writes the version's pack as a .zip at `out`, with its folders and a manifest. Pictures that
/// are byte for byte the dump's (still the disc's own) are left out. Returns (written, left out).
pub fn export(
    data_home: &Path,
    data: &Path,
    game: &str,
    version: &str,
    launcher_version: &str,
    out: &Path,
) -> Result<(u32, u32), String> {
    let id = id(game, version)?;
    let pack = pack_dir(data, &id);
    let dump = dump_dir(data_home, &id);
    let dumped: HashMap<String, PathBuf> =
        items_under(&dump).into_iter().filter(|i| !i.animated).map(|i| (i.hash, i.rel)).collect();
    let mut mine: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    let mut textures = 0;
    let mut left_out = 0;
    for item in user_items(&pack) {
        let mut files = Vec::new();
        for rel in item_files(&pack, &item) {
            files.push((rel.clone(), std::fs::read(pack.join(&rel)).map_err(|e| format!("{}: {e}", rel.display()))?));
        }
        let unchanged = !item.animated
            && dumped
                .get(&item.hash)
                .and_then(|d| std::fs::read(dump.join(d)).ok())
                .is_some_and(|d| files.first().is_some_and(|(_, bytes)| *bytes == d));
        if unchanged {
            left_out += 1;
        } else {
            textures += 1;
            mine.extend(files);
        }
    }
    if textures == 0 {
        return Err(if left_out > 0 {
            "Every picture in your pack is still the game's own: edit some before sharing it.".into()
        } else {
            "Your pack has no pictures yet.".into()
        });
    }
    let manifest = Manifest {
        format: 1,
        game: Some(id.clone()),
        name: None,
        textures,
        made_with: Some(format!("OpenRAC launcher {launcher_version}")),
    };
    let write = || -> Result<(), String> {
        let file = std::fs::File::create(out).map_err(|e| format!("{}: {e}", out.display()))?;
        let mut zip = zip::ZipWriter::new(file);
        // PNGs are compressed already.
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
        zip.start_file(MANIFEST, stored).map_err(|e| e.to_string())?;
        zip.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        for (rel, bytes) in &mine {
            let name = rel.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join("/");
            zip.start_file(name, stored).map_err(|e| e.to_string())?;
            zip.write_all(bytes).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
        Ok(())
    };
    if let Err(e) = write() {
        let _ = std::fs::remove_file(out);
        return Err(e);
    }
    Ok((textures, left_out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const HASH_A: &str = "0123456789abcdef";
    const HASH_B: &str = "fedcba9876543210";
    const HASH_C: &str = "00000000000000c0";

    fn png(tag: u8) -> Vec<u8> {
        let mut bytes = PNG_MAGIC.to_vec();
        bytes.extend_from_slice(&[tag; 8]);
        bytes
    }

    fn make_zip(path: &Path, files: &[(&str, Vec<u8>)]) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        for (name, bytes) in files {
            zip.start_file(*name, o).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn manifest(game: &str) -> Vec<u8> {
        format!(r#"{{"format":1,"game":"{game}","name":"Shiny HUD"}}"#).into_bytes()
    }

    #[test]
    fn texture_names_are_sixteen_hex_digits() {
        assert_eq!(texture_name("0123456789ABCDEF.PNG").as_deref(), Some(HASH_A));
        assert_eq!(texture_name(&format!("{HASH_A}.png")).as_deref(), Some(HASH_A));
        for bad in ["0123456789abcde.png", "0123456789abcdeg.png", "0123456789abcdef.jpg", "hud.png", ".png"] {
            assert_eq!(texture_name(bad), None, "{bad}");
        }
        assert!(id("rac2", "ntsc").is_ok_and(|i| i == "rac2-ntsc"));
        assert!(id("../x", "ntsc").is_err());
    }

    #[test]
    fn portable_resampler_writes_a_real_rgba_png() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("source.png");
        let output = t.path().join("output.png");
        let pixels = vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 0];
        write_rgba_png(&source, 2, 2, &pixels).unwrap();
        portable_resample(&source, &output, 4, 4).unwrap();
        let (w, h, rgba) = png_rgba(&output).unwrap();
        assert_eq!((w, h), (4, 4));
        assert_eq!(rgba.len(), 4 * 4 * 4);
        assert_eq!(&rgba[..4], &[255, 0, 0, 255]);
        assert_eq!(&rgba[(3 * 4)..(4 * 4)], &[0, 255, 0, 255]);
        assert!(rgba.as_chunks::<4>().0.iter().any(|p| p[3] < 255));
    }

    #[test]
    fn a_mod_with_pictures_is_not_a_texture_pack() {
        let t = tempfile::tempdir().unwrap();
        let zip = t.path().join("mod.zip");
        make_zip(
            &zip,
            &[
                ("my-mod/openrac-mod.json", br#"{"id":"my-mod"}"#.to_vec()),
                ("my-mod/Interface/0123456789ABCDEF.png", png(1)),
            ],
        );
        let err = preview_import(&t.path().join("data"), "rac2", "ntsc", &zip).unwrap_err();
        assert_eq!(err, NOT_A_PACK_BUT_A_MOD);
    }

    #[test]
    fn a_good_pack_previews_and_imports_with_its_folders() {
        let t = tempfile::tempdir().unwrap();
        let data = t.path().join("data");
        let zip = t.path().join("pack.zip");
        make_zip(
            &zip,
            &[
                (MANIFEST, manifest("rac2-ntsc")),
                ("Interface/0123456789ABCDEF.png", png(1)),
                ("Planets/Oozla/fedcba9876543210.png", png(2)),
                ("Materials/0123456789abcdef/normal.png", png(3)),
                ("Materials/0123456789abcdef/roughness.png", png(4)),
                ("Materials/0123456789abcdef/height.png", png(5)),
                ("Materials/0123456789abcdef/ao.png", png(6)),
                ("Materials/0123456789abcdef/emissive.png", png(7)),
                ("Materials/0123456789abcdef/specular.png", png(8)),
                ("Materials/0123456789abcdef/metallic.png", png(9)),
                ("Materials/0123456789abcdef/cavity.png", png(10)),
                ("Materials/0123456789abcdef/curvature.png", png(11)),
                ("Materials/0123456789abcdef/edgewear.png", png(12)),
                ("Materials/0123456789abcdef/material.json", br#"{"roughnessScale":0.8}"#.to_vec()),
                ("README.txt", b"made by me".to_vec()),
            ],
        );
        let preview = preview_import(&data, "rac2", "ntsc", &zip).unwrap();
        assert_eq!((preview.textures, preview.current, preview.replacing, preview.skipped), (2, 0, 0, 1));
        assert_eq!(preview.name.as_deref(), Some("Shiny HUD"));
        let labels: Vec<_> = preview.folders.iter().map(|f| (f.label.as_str(), f.count)).collect();
        assert_eq!(labels, [("Interface", 1), ("Oozla", 1)]);
        let (count, kept) = import(&data, "rac2", "ntsc", &zip, ImportMode::Replace).unwrap();
        assert_eq!((count, kept), (2, None));
        let pack = pack_dir(&data, "rac2-ntsc");
        assert_eq!(fs::read(pack.join(format!("Interface/{HASH_A}.png"))).unwrap(), png(1));
        assert!(pack.join(format!("Planets/Oozla/{HASH_B}.png")).is_file());
        assert_eq!(fs::read(pack.join(format!("Materials/{HASH_A}/normal.png"))).unwrap(), png(3));
        assert_eq!(fs::read(pack.join(format!("Materials/{HASH_A}/roughness.png"))).unwrap(), png(4));
        for name in [
            "height.png",
            "ao.png",
            "emissive.png",
            "specular.png",
            "metallic.png",
            "cavity.png",
            "curvature.png",
            "edgewear.png",
        ] {
            assert!(pack.join(format!("Materials/{HASH_A}/{name}")).is_file(), "{name}");
        }
        assert!(pack.join(format!("Materials/{HASH_A}/material.json")).is_file());
        assert!(!pack.join("README.txt").exists());
        assert!(!data.join("texture_packs/.rac2-ntsc.importing").exists());
    }

    #[test]
    fn a_flat_pack_in_a_folder_of_its_own_imports_and_merges() {
        let t = tempfile::tempdir().unwrap();
        let data = t.path().join("data");
        let pack = pack_dir(&data, "rac2-ntsc");
        fs::create_dir_all(pack.join("Shared")).unwrap();
        fs::write(pack.join(format!("Shared/{HASH_A}.png")), png(9)).unwrap();
        fs::write(pack.join(format!("Shared/{HASH_C}.png")), png(9)).unwrap();
        let zip = t.path().join("flat.zip");
        // No manifest, one top folder, which is taken off.
        make_zip(&zip, &[("my pack/0123456789abcdef.png", png(1)), ("my pack/fedcba9876543210.png", png(2))]);
        let preview = preview_import(&data, "rac2", "ntsc", &zip).unwrap();
        assert_eq!((preview.textures, preview.current, preview.replacing), (2, 2, 1));
        assert_eq!(preview.folders[0].label, "Other pictures");
        import(&data, "rac2", "ntsc", &zip, ImportMode::Merge).unwrap();
        // The imported picture replaces the pack's own for that texture, wherever it was.
        assert!(!pack.join(format!("Shared/{HASH_A}.png")).exists());
        assert_eq!(fs::read(pack.join(format!("{HASH_A}.png"))).unwrap(), png(1));
        assert!(pack.join(format!("{HASH_B}.png")).is_file());
        assert!(pack.join(format!("Shared/{HASH_C}.png")).is_file());
        // A replace keeps the old pack beside it.
        let (_, kept) = import(&data, "rac2", "ntsc", &zip, ImportMode::Replace).unwrap();
        let kept = kept.unwrap();
        assert!(kept.join(format!("Shared/{HASH_C}.png")).is_file());
        assert!(!pack.join("Shared").exists());
        assert!(kept.file_name().unwrap().to_string_lossy().starts_with("rac2-ntsc (before import "));
    }

    #[test]
    fn material_sidecars_require_their_colour_texture() {
        let t = tempfile::tempdir().unwrap();
        let zip = t.path().join("orphan-material.zip");
        make_zip(&zip, &[(MANIFEST, manifest("rac2-ntsc")), ("Materials/0123456789abcdef/normal.png", png(3))]);
        let error = preview_import(t.path(), "rac2", "ntsc", &zip).unwrap_err();
        assert!(error.contains("has no colour texture"), "{error}");
    }

    #[test]
    fn another_games_pack_is_refused() {
        let t = tempfile::tempdir().unwrap();
        let zip = t.path().join("rac1.zip");
        make_zip(&zip, &[(MANIFEST, manifest("rac1-pal")), ("0123456789abcdef.png", png(1))]);
        let e = preview_import(t.path(), "rac2", "ntsc", &zip).unwrap_err();
        assert!(e.contains("Ratchet & Clank (PAL)") && e.contains("Going Commando"), "{e}");
        assert!(import(t.path(), "rac2", "ntsc", &zip, ImportMode::Merge).is_err());
        assert!(!pack_dir(t.path(), "rac2-ntsc").exists());
    }

    #[test]
    fn pictures_not_named_by_a_texture_are_refused() {
        let t = tempfile::tempdir().unwrap();
        let zip = t.path().join("bad.zip");
        make_zip(&zip, &[("0123456789abcdef.png", png(1)), ("Interface/health.png", png(2))]);
        let e = preview_import(t.path(), "rac2", "ntsc", &zip).unwrap_err();
        assert!(e.contains("Interface/health.png"), "{e}");
        let empty = t.path().join("empty.zip");
        make_zip(&empty, &[("readme.txt", b"hi".to_vec())]);
        assert!(preview_import(t.path(), "rac2", "ntsc", &empty).unwrap_err().contains("no texture pictures"));
        let twice = t.path().join("twice.zip");
        make_zip(&twice, &[("a/0123456789abcdef.png", png(1)), ("b/0123456789ABCDEF.png", png(2))]);
        assert!(preview_import(t.path(), "rac2", "ntsc", &twice).unwrap_err().contains("same texture"));
        let fake = t.path().join("fake.zip");
        make_zip(&fake, &[("0123456789abcdef.png", b"not a png".to_vec())]);
        assert!(import(t.path(), "rac2", "ntsc", &fake, ImportMode::Merge).unwrap_err().contains("not a PNG"));
        assert!(!pack_dir(t.path(), "rac2-ntsc").exists());
        let text = t.path().join("text.zip");
        fs::write(&text, "not a zip").unwrap();
        assert!(preview_import(t.path(), "rac2", "ntsc", &text).unwrap_err().contains("not a .zip"));
    }

    #[test]
    fn a_zip_reaching_outside_its_folder_is_refused() {
        let t = tempfile::tempdir().unwrap();
        for name in ["../0123456789abcdef.png", "Interface/../../0123456789abcdef.png", "/tmp/0123456789abcdef.png"] {
            let zip = t.path().join("evil.zip");
            make_zip(&zip, &[(name, png(1))]);
            let e = preview_import(&t.path().join("data"), "rac2", "ntsc", &zip).unwrap_err();
            assert!(e.contains("outside its folder"), "{name}: {e}");
            assert!(import(&t.path().join("data"), "rac2", "ntsc", &zip, ImportMode::Replace).is_err());
        }
        assert!(!t.path().join("0123456789abcdef.png").exists());
        assert!(!t.path().join("data").exists());
    }

    #[test]
    fn export_keeps_the_folders_and_leaves_the_discs_pictures_out() {
        let t = tempfile::tempdir().unwrap();
        let (home, data) = (t.path().join("home"), t.path().join("data"));
        let pack = pack_dir(&data, "rac2-ntsc");
        let dump = dump_dir(&home, "rac2-ntsc");
        fs::create_dir_all(pack.join("Planets/Oozla")).unwrap();
        fs::create_dir_all(pack.join("Interface")).unwrap();
        fs::create_dir_all(dump.join("Interface")).unwrap();
        fs::write(pack.join(format!("Planets/Oozla/{HASH_B}.png")), png(5)).unwrap();
        fs::write(pack.join(format!("Interface/{HASH_A}.png")), png(1)).unwrap();
        fs::write(dump.join(format!("Interface/{HASH_A}.png")), png(1)).unwrap(); // unchanged
        fs::create_dir_all(pack.join(ULTRA_FOLDER)).unwrap();
        fs::write(pack.join(ULTRA_FOLDER).join(format!("{HASH_C}.png")), png(7)).unwrap();
        fs::write(pack.join("notes.txt"), "mine").unwrap();
        let out = t.path().join("out.zip");
        assert_eq!(export(&home, &data, "rac2", "ntsc", "1.2.3", &out).unwrap(), (1, 1));
        let mut archive = open_zip(&out).unwrap();
        let names: Vec<String> = archive.file_names().map(|n| n.unwrap().into_owned()).collect();
        assert_eq!(names, [MANIFEST.to_string(), format!("Planets/Oozla/{HASH_B}.png")]);
        let mut text = String::new();
        archive.by_name(MANIFEST).unwrap().read_to_string(&mut text).unwrap();
        let m: Manifest = serde_json::from_str(&text).unwrap();
        assert_eq!((m.game.as_deref(), m.textures, m.format), (Some("rac2-ntsc"), 1, 1));
        // What it exported imports again, into another data folder, folders and all.
        let other = t.path().join("other");
        import(&other, "rac2", "ntsc", &out, ImportMode::Replace).unwrap();
        assert!(pack_dir(&other, "rac2-ntsc").join(format!("Planets/Oozla/{HASH_B}.png")).is_file());
        // A pack of nothing but the disc's pictures is not exported.
        fs::remove_file(pack.join(format!("Planets/Oozla/{HASH_B}.png"))).unwrap();
        fs::remove_file(&out).unwrap();
        assert!(export(&home, &data, "rac2", "ntsc", "1.2.3", &out).unwrap_err().contains("still the game's own"));
        assert!(!out.exists());
    }

    #[test]
    fn view_counts_folders_and_the_switches_write_their_files() {
        let t = tempfile::tempdir().unwrap();
        let (home, data) = (t.path().join("home"), t.path().join("data"));
        let dump = dump_dir(&home, "rac2-ntsc");
        for (dir, hash) in [
            ("Interface", HASH_A),
            ("Shared", HASH_B),
            ("Planets/Oozla", HASH_C),
            ("Planets/Endako", "00000000000000d0"),
        ] {
            fs::create_dir_all(dump.join(dir)).unwrap();
            fs::write(dump.join(dir).join(format!("{hash}.png")), png(1)).unwrap();
        }
        let v = view(&home, &data, "rac2", "ntsc").unwrap();
        assert!(v.enabled && !v.dumping);
        assert_eq!((v.dump_count, v.pack_count), (4, 0));
        let order: Vec<_> = v.dump.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(order, ["Interface", "Shared", "Planets/Endako", "Planets/Oozla"]);
        assert_eq!(play_env(&home, "rac2", "ntsc"), None);
        set_dumping(&home, "rac2", "ntsc", true).unwrap();
        assert_eq!(play_env(&home, "rac2", "ntsc").unwrap(), (DUMP_ENV.to_string(), dump.display().to_string()));
        set_enabled(&home, "rac2", "ntsc", false).unwrap();
        let settings = fs::read_to_string(settings_file(&home, "rac2", "ntsc")).unwrap();
        assert!(settings.contains("texture_pack = 0"), "{settings}");
        let v = view(&home, &data, "rac2", "ntsc").unwrap();
        assert!(!v.enabled && v.dumping);
        set_dumping(&home, "rac2", "ntsc", false).unwrap();
        set_dumping(&home, "rac2", "ntsc", false).unwrap();
        assert_eq!(play_env(&home, "rac2", "ntsc"), None);
        assert_eq!(folder(&home, &data, "rac2", "ntsc", true, "Planets/Oozla").unwrap(), dump.join("Planets/Oozla"));
        assert!(folder(&home, &data, "rac2", "ntsc", true, "../x").is_err());
        let pack = folder(&home, &data, "rac2", "ntsc", false, "").unwrap();
        assert!(pack.is_dir());

        // Offline Ultra has its own visible switch. Turning it off deletes only the generated
        // Ultra4K cache and leaves a player's hand-made/imported pack picture alone.
        let custom = pack.join(format!("{HASH_A}.png"));
        fs::write(&custom, png(9)).unwrap();
        let ultra = pack.join(ULTRA_FOLDER);
        fs::create_dir_all(&ultra).unwrap();
        let generated = ultra.join(format!("{HASH_B}.png"));
        fs::write(&generated, png(8)).unwrap();
        let bytes = fs::metadata(&generated).unwrap().len();
        let mut stamp = UltraStamp {
            algorithm: ULTRA_ALGORITHM,
            source_fingerprint: "test".into(),
            source_files: 7,
            textures: 1,
            skipped_custom: 1,
            bytes,
            max_edge: ULTRA_MAX_EDGE,
            scale: ULTRA_SCALE,
        };
        fs::write(ultra.join(ULTRA_STAMP), serde_json::to_vec(&stamp).unwrap()).unwrap();
        let v = view(&home, &data, "rac2", "ntsc").unwrap();
        assert!(v.ultra_enabled && v.ultra_ready);
        assert_eq!((v.ultra_textures, v.ultra_bytes, v.pack_count), (1, bytes, 1));
        assert!(v.pack.iter().all(|folder| folder.path != ULTRA_FOLDER));
        // A damaged/truncated generated cache is not reported as ready merely because its stamp exists.
        stamp.bytes += 1;
        fs::write(ultra.join(ULTRA_STAMP), serde_json::to_vec(&stamp).unwrap()).unwrap();
        assert!(!view(&home, &data, "rac2", "ntsc").unwrap().ultra_ready);
        stamp.bytes = bytes;
        fs::write(ultra.join(ULTRA_STAMP), serde_json::to_vec(&stamp).unwrap()).unwrap();
        assert!(view(&home, &data, "rac2", "ntsc").unwrap().ultra_ready);
        set_ultra_enabled(&home, &data, "rac2", "ntsc", false).unwrap();
        assert!(custom.is_file());
        assert!(ultra.exists());
        let v = view(&home, &data, "rac2", "ntsc").unwrap();
        assert!(!v.ultra_enabled && v.ultra_ready);
        delete_ultra(&data, "rac2", "ntsc").unwrap();
        assert!(!ultra.exists() && custom.is_file());
        set_ultra_enabled(&home, &data, "rac2", "ntsc", true).unwrap();
        assert!(view(&home, &data, "rac2", "ntsc").unwrap().ultra_enabled);
    }

    #[test]
    fn replacing_a_pack_keeps_the_generated_ultra_cache_out_of_the_backup() {
        let t = tempfile::tempdir().unwrap();
        let data = t.path().join("data");
        let pack = pack_dir(&data, "rac2-ntsc");
        fs::create_dir_all(pack.join(ULTRA_FOLDER)).unwrap();
        fs::write(pack.join(format!("{HASH_A}.png")), png(1)).unwrap();
        fs::write(pack.join(ULTRA_FOLDER).join(format!("{HASH_B}.png")), png(8)).unwrap();
        let zip = t.path().join("replacement.zip");
        make_zip(&zip, &[(MANIFEST, manifest("rac2-ntsc")), (&format!("{HASH_C}.png"), png(3))]);

        let (_, kept) = import(&data, "rac2", "ntsc", &zip, ImportMode::Replace).unwrap();
        let kept = kept.unwrap();
        assert!(pack.join(format!("{HASH_C}.png")).is_file());
        assert!(pack.join(ULTRA_FOLDER).join(format!("{HASH_B}.png")).is_file());
        assert!(!kept.join(ULTRA_FOLDER).exists());
        assert!(kept.join(format!("{HASH_A}.png")).is_file());
    }

    #[test]
    fn animated_textures_import_count_and_export_with_their_files() {
        let t = tempfile::tempdir().unwrap();
        let (home, data) = (t.path().join("home"), t.path().join("data"));
        let zip = t.path().join("anim.zip");
        make_zip(
            &zip,
            &[
                (MANIFEST, manifest("rac2-ntsc")),
                ("Interface/0123456789ABCDEF/0001.png", png(1)),
                ("Interface/0123456789ABCDEF/0002.png", png(2)),
                ("Interface/0123456789ABCDEF/animation.json", br#"{"fps": 8, "mode": "pingpong"}"#.to_vec()),
                ("Shared/fedcba9876543210.gif", b"GIF89a....".to_vec()),
                // The dump's grouping of a game's own animation: pictures named by a hash, each a
                // texture of its own.
                ("Animated/00000000000000c0/00000000000000c0.png", png(3)),
                ("Animated/00000000000000c0/00000000000000d0.png", png(4)),
            ],
        );
        let preview = preview_import(&data, "rac2", "ntsc", &zip).unwrap();
        assert_eq!((preview.textures, preview.animated), (4, 2));
        import(&data, "rac2", "ntsc", &zip, ImportMode::Replace).unwrap();
        let pack = pack_dir(&data, "rac2-ntsc");
        assert!(pack.join(format!("Interface/{HASH_A}/0002.png")).is_file());
        assert!(pack.join(format!("Interface/{HASH_A}/animation.json")).is_file());
        assert!(pack.join(format!("Shared/{HASH_B}.gif")).is_file());
        let v = view(&home, &data, "rac2", "ntsc").unwrap();
        assert_eq!((v.pack_count, v.pack_animated), (4, 2));
        // A merge replaces an animation whole: the old frames go.
        let again = t.path().join("again.zip");
        make_zip(&again, &[(&format!("Interface/{HASH_A}/0001.png"), png(7))]);
        import(&data, "rac2", "ntsc", &again, ImportMode::Merge).unwrap();
        assert!(!pack.join(format!("Interface/{HASH_A}/0002.png")).exists());
        assert!(!pack.join(format!("Interface/{HASH_A}/animation.json")).exists());
        // Export keeps the folders, the frames and the settings.
        import(&data, "rac2", "ntsc", &zip, ImportMode::Merge).unwrap();
        let out = t.path().join("out.zip");
        assert_eq!(export(&home, &data, "rac2", "ntsc", "1", &out).unwrap(), (4, 0));
        let archive = open_zip(&out).unwrap();
        let names: Vec<String> = archive.file_names().map(|n| n.unwrap().into_owned()).collect();
        for want in [
            "Interface/0123456789abcdef/0001.png",
            "Interface/0123456789abcdef/animation.json",
            "Shared/fedcba9876543210.gif",
            "Animated/00000000000000c0/00000000000000d0.png",
        ] {
            assert!(names.iter().any(|n| n == want), "{want} in {names:?}");
        }
        // An animation folder without frames, or a GIF that is not one, is refused.
        let empty = t.path().join("empty.zip");
        make_zip(&empty, &[(&format!("{HASH_A}/animation.json"), b"{}".to_vec())]);
        assert!(preview_import(&data, "rac2", "ntsc", &empty).unwrap_err().contains("no frames"));
        let gif = t.path().join("gif.zip");
        make_zip(&gif, &[(&format!("{HASH_A}.gif"), png(1))]);
        assert!(import(&data, "rac2", "ntsc", &gif, ImportMode::Merge).unwrap_err().contains("not a GIF"));
    }
}
