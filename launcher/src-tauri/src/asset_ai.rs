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
            output: data.join("prepared-graphics").join("meshes").join(version).display().to_string(),
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

    let overrides = data.join("prepared-graphics").join("meshes").join(version);
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

fn validate_helper_pack(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut render_meshes = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| format!("could not read {}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let ty = entry.file_type().map_err(|e| e.to_string())?;
            if ty.is_symlink() {
                return Err(format!("geometry helper produced a symlink: {}", path.display()));
            }
            if ty.is_dir() {
                stack.push(path);
                continue;
            }
            if is_collision(&path) {
                return Err(format!("geometry helper attempted to replace gameplay collision: {}", path.display()));
            }
            if is_glb(&path) {
                render_meshes.push(path);
            }
        }
    }
    render_meshes.sort();
    Ok(render_meshes)
}

/// Inputs for one consented Graphics Preparation geometry run.
pub(crate) struct GraphicsHelperBake<'a> {
    pub python: &'a Path,
    pub openrac_root: &'a Path,
    pub helper: &'a Path,
    pub data: &'a Path,
    pub game: &'a str,
    pub version: &'a str,
    pub material_pack: Option<&'a Path>,
    pub options: serde_json::Value,
}

/// Runs the Geometry Enhancement helper shipped inside the consented graphics-preparation mod.
///
/// The helper exports OpenRAC's render assets, refines them offline, then imports a normal
/// level_NN override tree. Work happens under <data>/ai_assets first; only a validated complete
/// pack replaces <data>/prepared-graphics/meshes/<version>. The game already mounts that directory
/// through --prepared-meshes on its next start.
pub(crate) fn bake_with_graphics_helper(request: GraphicsHelperBake<'_>) -> Result<MeshBake, String> {
    let GraphicsHelperBake { python, openrac_root, helper, data, game, version, material_pack, options } = request;
    if !helper.is_file() {
        return Err(format!("installed geometry helper is missing: {}", helper.display()));
    }
    if !openrac_root.join("editor/assets.py").is_file() {
        return Err(format!("OpenRAC asset tools are missing from {}", openrac_root.display()));
    }

    let levels = data.join("port").join(version);
    if !levels.is_dir() {
        return Err(format!("extracted levels are missing: {}", levels.display()));
    }
    let discovered = collect_glbs(&levels)?.len();

    let ai_root = data.join("ai_assets").join(version);
    let staging = ai_root.join(".geometry-helper-next");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| format!("could not create {}: {e}", staging.display()))?;

    let job_file = staging.join("job.json");
    let output_root = staging.join("output");
    let result_file = staging.join("result.json");
    let job = serde_json::json!({
        "format": 1,
        "kind": "openrac-graphics-preparation-job",
        "operation": "ai.meshes",
        "game": game,
        "version": version,
        "options": options,
    });
    fs::write(&job_file, serde_json::to_vec_pretty(&job).map_err(|e| e.to_string())?)
        .map_err(|e| format!("could not write {}: {e}", job_file.display()))?;

    let mut command = Command::new(python);
    command
        .arg(helper)
        .arg(&job_file)
        .arg("--openrac-root")
        .arg(openrac_root)
        .arg("--levels")
        .arg(&levels)
        .arg("--out")
        .arg(&output_root)
        .arg("--result-json")
        .arg(&result_file);
    if let Some(pack) = material_pack.filter(|pack| pack.join("Materials").is_dir()) {
        command.arg("--material-pack").arg(pack);
    }
    let run = command
        .current_dir(openrac_root)
        .output()
        .map_err(|e| format!("could not start geometry helper with {}: {e}", python.display()))?;

    if !run.status.success() {
        let detail = fs::read_to_string(&result_file)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|result| result["message"].as_str().map(str::to_owned))
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| {
                let stderr = String::from_utf8_lossy(&run.stderr);
                let stdout = String::from_utf8_lossy(&run.stdout);
                let text = if stderr.trim().is_empty() { stdout.trim() } else { stderr.trim() };
                if text.is_empty() {
                    format!("geometry helper failed with {}", run.status)
                } else {
                    text.chars().take(4096).collect()
                }
            });
        let _ = fs::remove_dir_all(&staging);
        return Err(detail);
    }

    let pack = output_root.join("pack");
    if !pack.is_dir() {
        let _ = fs::remove_dir_all(&staging);
        return Err("geometry helper completed without an override pack".into());
    }
    let outputs = validate_helper_pack(&pack)?;
    if outputs.is_empty() {
        let _ = fs::remove_dir_all(&staging);
        return Err("geometry helper produced no render meshes".into());
    }

    let target = data.join("prepared-graphics").join("meshes").join(version);
    let target_parent = target.parent().ok_or("prepared mesh path has no parent")?;
    fs::create_dir_all(target_parent).map_err(|e| format!("could not create {}: {e}", target_parent.display()))?;

    let backup = if target.exists() {
        let backup = ai_root.join("backups").join(format!("before-geometry-helper-{}", timestamp()?));
        if backup.exists() {
            let _ = fs::remove_dir_all(&staging);
            return Err(format!("geometry backup already exists: {}", backup.display()));
        }
        if let Some(parent) = backup.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("could not create {}: {e}", parent.display()))?;
        }
        fs::rename(&target, &backup).map_err(|e| format!("could not back up {}: {e}", target.display()))?;
        Some(backup)
    } else {
        None
    };

    if let Err(error) = fs::rename(&pack, &target) {
        if let Some(saved) = &backup {
            let _ = fs::rename(saved, &target);
        }
        let _ = fs::remove_dir_all(&staging);
        return Err(format!("could not activate {}: {error}", target.display()));
    }

    let processed = outputs.len();
    let backup_text = backup.as_ref().map(|path| path.display().to_string()).unwrap_or_default();
    let _ = fs::remove_dir_all(&staging);
    Ok(MeshBake {
        available: true,
        discovered,
        processed,
        backend: helper.display().to_string(),
        output: target.display().to_string(),
        backup: backup_text,
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
