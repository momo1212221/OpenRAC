// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
//! Offline AI render-mesh replacement orchestration.
//!
//! The game already loads visual mesh overrides from <data>/mods/<version>/level_NN before the
//! extracted level. This module stages a complete neural mesh pass outside that live tree, validates
//! that the worker produced only matching GLB render assets, then activates them with backups.
//! Gameplay collision is deliberately never offered to the worker.

use serde::Serialize;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshBake {
    pub available: bool,
    pub discovered: usize,
    pub processed: usize,
    pub backend: String,
    pub output: String,
    pub backup: String,
}

fn is_glb(path: &Path) -> bool {
    path.extension().and_then(OsStr::to_str).is_some_and(|ext| ext.eq_ignore_ascii_case("glb"))
}

fn is_collision(path: &Path) -> bool {
    path.file_name().and_then(OsStr::to_str).is_some_and(|name| name.eq_ignore_ascii_case("collision.glb"))
}

fn collect_glbs(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| format!("could not read {}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let ty = entry.file_type().map_err(|e| e.to_string())?;
            if ty.is_symlink() {
                continue;
            }
            if ty.is_dir() {
                stack.push(path);
            } else if is_glb(&path) && !is_collision(&path) {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

fn worker() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OPENRAC_MESH_AI").map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }

    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) { "openrac-mesh-ai.exe" } else { "openrac-mesh-ai" };
    let sibling = exe.parent()?.join(name);
    sibling.is_file().then_some(sibling)
}

fn timestamp() -> Result<u64, String> {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).map_err(|e| e.to_string())
}

fn restore(activated: &[(PathBuf, Option<PathBuf>)]) {
    for (target, backup) in activated.iter().rev() {
        if let Some(backup) = backup {
            let _ = fs::copy(backup, target);
        } else {
            let _ = fs::remove_file(target);
        }
    }
}

/// Runs the optional batch mesh-AI worker. Its contract is intentionally small:
///
/// openrac-mesh-ai --input-root LEVELS --output-root STAGING
///                 --mode faithful|balanced|enhanced --preserve-skinning
///
/// The worker must preserve relative paths and emit only replacement GLBs. A topology-changing
/// worker is expected to leave skinned assets untouched unless it can preserve joints/weights.
pub fn bake_meshes(data: &Path, version: &str, mode: &str) -> Result<MeshBake, String> {
    if !matches!(mode, "faithful" | "balanced" | "enhanced") {
        return Err(format!("unknown AI asset mode: {mode}"));
    }

    let levels = data.join("port").join(version);
    if !levels.is_dir() {
        return Err(format!("extracted levels are missing: {}", levels.display()));
    }
    let source = collect_glbs(&levels)?;
    let discovered = source.len();
    let Some(worker) = worker() else {
        return Ok(MeshBake {
            available: false,
            discovered,
            processed: 0,
            backend: String::new(),
            output: data.join("mods").join(version).display().to_string(),
            backup: String::new(),
        });
    };

    let ai_root = data.join("ai_assets").join(version);
    let staging = ai_root.join(".mesh-next");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| format!("could not create {}: {e}", staging.display()))?;

    let status = Command::new(&worker)
        .arg("--input-root")
        .arg(&levels)
        .arg("--output-root")
        .arg(&staging)
        .arg("--mode")
        .arg(mode)
        .arg("--preserve-skinning")
        .status()
        .map_err(|e| format!("could not start 3D AI backend {}: {e}", worker.display()))?;
    if !status.success() {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!("3D AI backend failed with {status}"));
    }

    let outputs = collect_glbs(&staging)?;
    if discovered > 0 && outputs.is_empty() {
        let _ = fs::remove_dir_all(&staging);
        return Err("3D AI backend produced no render meshes".into());
    }

    let mut relative = Vec::with_capacity(outputs.len());
    for output in &outputs {
        let rel = output.strip_prefix(&staging).map_err(|e| e.to_string())?;
        if rel.components().any(|c| matches!(c, std::path::Component::ParentDir))
            || is_collision(rel)
            || !levels.join(rel).is_file()
        {
            let _ = fs::remove_dir_all(&staging);
            return Err(format!("3D AI backend produced an unexpected asset: {}", rel.display()));
        }
        relative.push(rel.to_path_buf());
    }

    let overrides = data.join("mods").join(version);
    let backup_root = ai_root.join("backups").join(format!("before-mesh-ai-{}", timestamp()?));
    let mut activated: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();

    for rel in &relative {
        let from = staging.join(rel);
        let target = overrides.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                restore(&activated);
                format!("could not create {}: {e}", parent.display())
            })?;
        }

        let backup = if target.is_file() {
            let saved = backup_root.join(rel);
            if let Some(parent) = saved.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    restore(&activated);
                    format!("could not create backup folder {}: {e}", parent.display())
                })?;
            }
            fs::copy(&target, &saved).map_err(|e| {
                restore(&activated);
                format!("could not back up {}: {e}", target.display())
            })?;
            Some(saved)
        } else {
            None
        };

        if let Err(e) = fs::copy(&from, &target) {
            restore(&activated);
            return Err(format!("could not activate {}: {e}", target.display()));
        }
        activated.push((target, backup));
    }

    let _ = fs::remove_dir_all(&staging);
    Ok(MeshBake {
        available: true,
        discovered,
        processed: relative.len(),
        backend: worker.display().to_string(),
        output: overrides.display().to_string(),
        backup: if backup_root.is_dir() { backup_root.display().to_string() } else { String::new() },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_is_never_a_render_asset() {
        assert!(is_collision(Path::new("collision.glb")));
        assert!(!is_collision(Path::new("terrain.glb")));
    }

    #[test]
    fn modes_are_bounded() {
        let bad = Path::new("/definitely/missing");
        assert!(bake_meshes(bad, "pal", "wild").unwrap_err().contains("unknown AI asset mode"));
    }
}
