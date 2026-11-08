// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors

//! Provider-neutral cloud saves backed by a folder the player owns.
//!
//! OpenRAC never hosts the files. The player chooses a folder that may be synced by OneDrive,
//! Dropbox, iCloud Drive, Syncthing, a NAS, or any other file synchronizer. The game's memory card
//! stays local. This module writes immutable OpenRAC save archives to that folder and remembers the
//! last snapshot this device saw, so two devices changing the same card become a conflict instead
//! of silently overwriting one another.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{save_transfer, saves};

const CONFIG_FILE: &str = "cloud-saves.json";
const STATE_FILE: &str = "cloud-saves-state.json";
const CLOUD_FOLDER: &str = "OpenRAC Cloud Saves";
const FORMAT_VERSION: u32 = 1;
static CLOUD_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CloudConfig {
    #[serde(default)]
    folder: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DeviceStateFile {
    #[serde(default)]
    folder: Option<String>,
    #[serde(default)]
    games: BTreeMap<String, DeviceGameState>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DeviceGameState {
    #[serde(default)]
    last_snapshot: Option<String>,
    #[serde(default)]
    local_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotMeta {
    format_version: u32,
    id: String,
    #[serde(default)]
    parents: Vec<String>,
    serial: String,
    created: String,
    device: String,
    content_sha256: String,
    archive_sha256: String,
    launcher_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudConfigView {
    pub folder: Option<String>,
    pub cloud_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSnapshotInfo {
    pub id: String,
    pub parents: Vec<String>,
    pub created: String,
    pub device: String,
    pub content_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSaveStatus {
    pub enabled: bool,
    pub folder: Option<String>,
    pub cloud_dir: Option<String>,
    pub state: String,
    pub snapshots: usize,
    pub heads: Vec<CloudSnapshotInfo>,
    pub latest: Option<CloudSnapshotInfo>,
    pub local_has_saves: bool,
    pub local_changed: bool,
    pub cloud_changed: bool,
    pub conflict: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncResult {
    pub action: String,
    pub message: String,
    pub status: CloudSaveStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    Auto,
    UseLocal,
    UseCloud,
}

impl Strategy {
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("auto") {
            "auto" => Ok(Self::Auto),
            "use-local" => Ok(Self::UseLocal),
            "use-cloud" => Ok(Self::UseCloud),
            other => Err(format!("{other:?} is not a cloud save sync strategy")),
        }
    }
}

fn config_path() -> PathBuf {
    saves::data_home().join(CONFIG_FILE)
}

fn state_path() -> PathBuf {
    saves::data_home().join(STATE_FILE)
}

fn plain_serial(serial: &str) -> bool {
    !serial.is_empty()
        && serial != "."
        && serial != ".."
        && !serial.contains("..")
        && serial.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let tmp = PathBuf::from(format!("{}.tmp", path.to_string_lossy()));
    fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

fn selected_folder() -> Option<PathBuf> {
    read_json::<CloudConfig>(&config_path()).folder
}

fn cloud_dir(folder: &Path) -> PathBuf {
    folder.join(CLOUD_FOLDER).join(format!("v{FORMAT_VERSION}"))
}

fn game_dir(folder: &Path, serial: &str) -> Result<PathBuf, String> {
    if !plain_serial(serial) {
        return Err(format!("{serial:?} is not a disc serial"));
    }
    Ok(cloud_dir(folder).join(serial))
}

fn snapshots_dir(folder: &Path, serial: &str) -> Result<PathBuf, String> {
    Ok(game_dir(folder, serial)?.join("snapshots"))
}

fn objects_dir(folder: &Path, serial: &str) -> Result<PathBuf, String> {
    Ok(game_dir(folder, serial)?.join("objects"))
}

pub fn config() -> CloudConfigView {
    let folder = selected_folder();
    CloudConfigView {
        folder: folder.as_ref().map(|p| p.display().to_string()),
        cloud_dir: folder.as_ref().map(|p| cloud_dir(p).display().to_string()),
    }
}

pub fn set_folder(folder: Option<&str>) -> Result<CloudConfigView, String> {
    let _guard = CLOUD_LOCK.lock().map_err(|_| "cloud saves are busy".to_string())?;
    let previous = selected_folder();
    let folder = folder.map(str::trim).filter(|s| !s.is_empty()).map(PathBuf::from);
    if let Some(path) = &folder {
        if !path.is_dir() {
            return Err(format!("{} is not a folder", path.display()));
        }
        let dir = cloud_dir(path);
        fs::create_dir_all(&dir).map_err(|e| format!("cannot use {}: {e}", dir.display()))?;
        let probe = dir.join(".write-test");
        fs::write(&probe, b"openrac").map_err(|e| format!("cannot write to {}: {e}", dir.display()))?;
        let _ = fs::remove_file(probe);
    }
    write_json(&config_path(), &CloudConfig { folder: folder.clone() })?;
    // A different provider has a different history. Keeping a stale last-snapshot pointer would
    // make its first sync look safe when it is not. Re-selecting the same folder keeps the device's
    // last-seen snapshot so a normal local edit does not become a false conflict.
    if previous != folder {
        write_json(&state_path(), &DeviceStateFile::default())?;
    }
    Ok(CloudConfigView {
        folder: folder.as_ref().map(|p| p.display().to_string()),
        cloud_dir: folder.as_ref().map(|p| cloud_dir(p).display().to_string()),
    })
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// A stable identity for the card contents. The export's timestamp and launcher version are not
/// part of it, so checking an unchanged card twice answers the same value.
fn manifest_fingerprint(manifest: &save_transfer::SaveArchiveManifest) -> String {
    let mut hash = Sha256::new();
    hash.update(manifest.serial.as_bytes());
    hash.update(b"\0");
    for file in &manifest.files {
        hash.update(file.path.as_bytes());
        hash.update(b"\0");
        hash.update(file.size.to_le_bytes());
        hash.update(b"\0");
        hash.update(file.sha256.as_bytes());
        hash.update(b"\n");
    }
    format!("{:x}", hash.finalize())
}

fn working_archive(serial: &str) -> Result<PathBuf, String> {
    if !plain_serial(serial) {
        return Err(format!("{serial:?} is not a disc serial"));
    }
    let dir = saves::data_home().join("cloud-saves-work");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    Ok(dir.join(format!("{serial}-{}-{nonce}.zip", std::process::id())))
}

fn local_archive(
    serial: &str,
    launcher_version: &str,
) -> Result<Option<(PathBuf, save_transfer::SaveArchiveManifest, String)>, String> {
    let status = saves::inspect_saves(serial);
    if !status.slots.iter().any(|slot| !slot.is_empty) {
        return Ok(None);
    }
    let path = working_archive(serial)?;
    let manifest = match save_transfer::export_saves(serial, None, launcher_version, &path) {
        Ok(manifest) => manifest,
        Err(e) => {
            let _ = fs::remove_file(&path);
            return Err(e);
        }
    };
    let fingerprint = manifest_fingerprint(&manifest);
    Ok(Some((path, manifest, fingerprint)))
}

fn valid_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn read_meta(path: &Path) -> Option<SnapshotMeta> {
    let text = fs::read_to_string(path).ok()?;
    let meta: SnapshotMeta = serde_json::from_str(&text).ok()?;
    let stem = path.file_stem()?.to_string_lossy();
    if meta.format_version != FORMAT_VERSION
        || !plain_serial(&meta.serial)
        || !valid_hex(&meta.id)
        || !valid_hex(&meta.content_sha256)
        || !valid_hex(&meta.archive_sha256)
        || stem != meta.id
        || meta.parents.iter().any(|p| !valid_hex(p))
    {
        return None;
    }
    Some(meta)
}

fn list_meta(folder: &Path, serial: &str) -> Result<Vec<SnapshotMeta>, String> {
    let dir = snapshots_dir(folder, serial)?;
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else { return Ok(out) };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            if let Some(meta) = read_meta(&path).filter(|m| m.serial.eq_ignore_ascii_case(serial)) {
                out.push(meta);
            }
        }
    }
    out.sort_by(|a, b| (a.created.as_str(), a.id.as_str()).cmp(&(b.created.as_str(), b.id.as_str())));
    Ok(out)
}

/// The tips of the immutable snapshot graph. Concurrent devices may make two children of the same
/// parent; both remain tips and are reported as a conflict until a later snapshot names both as
/// parents.
fn heads(snapshots: &[SnapshotMeta]) -> Vec<&SnapshotMeta> {
    let parents: BTreeSet<&str> = snapshots.iter().flat_map(|s| s.parents.iter().map(String::as_str)).collect();
    let mut out: Vec<&SnapshotMeta> = snapshots.iter().filter(|s| !parents.contains(s.id.as_str())).collect();
    out.sort_by(|a, b| (a.created.as_str(), a.id.as_str()).cmp(&(b.created.as_str(), b.id.as_str())));
    out
}

fn info(meta: &SnapshotMeta) -> CloudSnapshotInfo {
    CloudSnapshotInfo {
        id: meta.id.clone(),
        parents: meta.parents.clone(),
        created: meta.created.clone(),
        device: meta.device.clone(),
        content_sha256: meta.content_sha256.clone(),
    }
}

fn load_state(folder: &Path) -> DeviceStateFile {
    let wanted = cloud_dir(folder).display().to_string();
    let state: DeviceStateFile = read_json(&state_path());
    if state.folder.as_deref() == Some(wanted.as_str()) {
        state
    } else {
        DeviceStateFile { folder: Some(wanted), games: BTreeMap::new() }
    }
}

fn save_state(folder: &Path, state: &DeviceStateFile) -> Result<(), String> {
    let mut state = state.clone();
    state.folder = Some(cloud_dir(folder).display().to_string());
    write_json(&state_path(), &state)
}

fn device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "OpenRAC device".into())
}

fn snapshot_id(serial: &str, content: &str, parents: &[String], device: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(serial.as_bytes());
    hash.update(b"\0");
    hash.update(content.as_bytes());
    hash.update(b"\0");
    for parent in parents {
        hash.update(parent.as_bytes());
        hash.update(b"\n");
    }
    hash.update(device.as_bytes());
    hash.update(b"\0");
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    hash.update(nonce.to_le_bytes());
    format!("{:x}", hash.finalize())
}

type LocalSnapshot = (PathBuf, save_transfer::SaveArchiveManifest, String);

fn status_with(
    folder: Option<&Path>,
    serial: &str,
    launcher_version: &str,
) -> Result<(CloudSaveStatus, Option<LocalSnapshot>), String> {
    let Some(folder) = folder else {
        return Ok((
            CloudSaveStatus {
                enabled: false,
                folder: None,
                cloud_dir: None,
                state: "disabled".into(),
                snapshots: 0,
                heads: Vec::new(),
                latest: None,
                local_has_saves: saves::inspect_saves(serial).slots.iter().any(|slot| !slot.is_empty),
                local_changed: false,
                cloud_changed: false,
                conflict: false,
            },
            None,
        ));
    };

    let snapshots = list_meta(folder, serial)?;
    let tips = heads(&snapshots);
    let local = local_archive(serial, launcher_version)?;
    let local_content = local.as_ref().map(|(_, _, id)| id.as_str());

    let mut state_file = load_state(folder);
    let device = state_file.games.entry(serial.to_string()).or_default();

    // A fresh device whose local card exactly matches the one unambiguous cloud head can adopt it
    // without creating a redundant snapshot or a false conflict.
    if tips.len() == 1
        && local_content == Some(tips[0].content_sha256.as_str())
        && (device.last_snapshot.is_none() || device.local_fingerprint.is_none())
    {
        device.last_snapshot = Some(tips[0].id.clone());
        device.local_fingerprint = local_content.map(str::to_string);
        save_state(folder, &state_file)?;
    }

    let device = state_file.games.get(serial).cloned().unwrap_or_default();
    let local_has_saves = local.is_some();
    let local_changed = match (local_content, device.local_fingerprint.as_deref()) {
        (None, None) => false,
        (Some(now), Some(before)) => now != before,
        (Some(_), None) => true,
        (None, Some(_)) => true,
    };
    let cloud_changed = match tips.as_slice() {
        [] => device.last_snapshot.is_some(),
        [only] => device.last_snapshot.as_deref() != Some(only.id.as_str()),
        _ => true,
    };
    let conflict = tips.len() > 1 || (local_changed && cloud_changed && local_has_saves && !tips.is_empty());
    let state = if !local_has_saves && tips.is_empty() {
        "empty"
    } else if conflict {
        "conflict"
    } else if cloud_changed {
        "cloud-changes"
    } else if local_changed {
        "local-changes"
    } else {
        "synced"
    };
    let latest = tips.last().map(|m| info(m));
    Ok((
        CloudSaveStatus {
            enabled: true,
            folder: Some(folder.display().to_string()),
            cloud_dir: Some(cloud_dir(folder).display().to_string()),
            state: state.into(),
            snapshots: snapshots.len(),
            heads: tips.into_iter().map(info).collect(),
            latest,
            local_has_saves,
            local_changed,
            cloud_changed,
            conflict,
        },
        local,
    ))
}

fn status_unlocked(serial: &str, launcher_version: &str) -> Result<CloudSaveStatus, String> {
    let folder = selected_folder();
    let (status, local) = status_with(folder.as_deref(), serial, launcher_version)?;
    if let Some((path, _, _)) = local {
        let _ = fs::remove_file(path);
    }
    Ok(status)
}

pub fn status(serial: &str, launcher_version: &str) -> Result<CloudSaveStatus, String> {
    let _guard = CLOUD_LOCK.lock().map_err(|_| "cloud saves are busy".to_string())?;
    status_unlocked(serial, launcher_version)
}

fn upload(
    folder: &Path,
    serial: &str,
    launcher_version: &str,
    local: Option<(PathBuf, save_transfer::SaveArchiveManifest, String)>,
) -> Result<String, String> {
    let (work, manifest, content) = match local {
        Some(local) => local,
        None => local_archive(serial, launcher_version)?.ok_or("there are no local saves to upload")?,
    };
    let result = (|| {
        let snapshots = list_meta(folder, serial)?;
        let tips = heads(&snapshots);
        if tips.len() == 1 && tips[0].content_sha256 == content {
            let mut state = load_state(folder);
            state.games.insert(
                serial.to_string(),
                DeviceGameState { last_snapshot: Some(tips[0].id.clone()), local_fingerprint: Some(content.clone()) },
            );
            save_state(folder, &state)?;
            return Ok(tips[0].id.clone());
        }

        let object_dir = objects_dir(folder, serial)?;
        let snap_dir = snapshots_dir(folder, serial)?;
        fs::create_dir_all(&object_dir).map_err(|e| format!("{}: {e}", object_dir.display()))?;
        fs::create_dir_all(&snap_dir).map_err(|e| format!("{}: {e}", snap_dir.display()))?;
        let object = object_dir.join(format!("{content}.zip"));
        if object.exists() {
            // A content-addressed object must actually contain that content. A half-synced or
            // damaged object is never silently accepted or overwritten.
            let checked = save_transfer::read_archive(&object)
                .map_err(|e| format!("{} is not a usable cloud save object: {e}", object.display()))?;
            if manifest_fingerprint(&checked.manifest) != content {
                return Err(format!("{} does not match its content id; nothing was overwritten", object.display()));
            }
        } else {
            // The work archive lives in OpenRAC's local data folder. The chosen cloud folder may
            // be another filesystem (external drive, NAS), where rename(work, object) would fail.
            // Copy to a temporary file inside the destination, verify it, then rename there.
            let partial = object_dir.join(format!(".{content}.zip.partial-{}", std::process::id()));
            let copied = (|| -> Result<(), String> {
                fs::copy(&work, &partial).map_err(|e| format!("{}: {e}", partial.display()))?;
                let checked = save_transfer::read_archive(&partial)
                    .map_err(|e| format!("{} could not be verified after copying: {e}", partial.display()))?;
                if manifest_fingerprint(&checked.manifest) != content {
                    return Err(format!("{} changed while it was copied; nothing was published", partial.display()));
                }
                fs::rename(&partial, &object).map_err(|e| format!("{}: {e}", object.display()))
            })();
            if copied.is_err() {
                let _ = fs::remove_file(&partial);
            }
            copied?;
        }
        let archive_sha256 = file_sha256(&object)?;
        let parents: Vec<String> = tips.iter().map(|m| m.id.clone()).collect();
        let device = device_name();
        let id = snapshot_id(serial, &content, &parents, &device);
        let meta = SnapshotMeta {
            format_version: FORMAT_VERSION,
            id: id.clone(),
            parents,
            serial: serial.to_string(),
            created: manifest.created,
            device,
            content_sha256: content.clone(),
            archive_sha256,
            launcher_version: launcher_version.to_string(),
        };
        write_json(&snap_dir.join(format!("{id}.json")), &meta)?;
        let mut state = load_state(folder);
        state.games.insert(
            serial.to_string(),
            DeviceGameState { last_snapshot: Some(id.clone()), local_fingerprint: Some(content) },
        );
        save_state(folder, &state)?;
        Ok::<_, String>(id)
    })();
    let _ = fs::remove_file(work);
    result
}

fn download(folder: &Path, serial: &str, head: &SnapshotMeta) -> Result<(), String> {
    let archive = objects_dir(folder, serial)?.join(format!("{}.zip", head.content_sha256));
    if file_sha256(&archive)? != head.archive_sha256.to_ascii_lowercase() {
        return Err(format!("{} changed or is damaged; nothing was restored", archive.display()));
    }
    // read_archive verifies the manifest, every path, size and per-file SHA-256 before import_saves
    // backs up the current card and replaces it.
    let checked = save_transfer::read_archive(&archive)?;
    if !checked.manifest.serial.eq_ignore_ascii_case(serial) {
        return Err(format!("the cloud snapshot is for {}, not {serial}", checked.manifest.serial));
    }
    let choice = save_transfer::SaveImportChoice { replace_all: true, slots: Vec::new() };
    save_transfer::import_saves(serial, &archive, &choice)?;
    let mut state = load_state(folder);
    state.games.insert(
        serial.to_string(),
        DeviceGameState { last_snapshot: Some(head.id.clone()), local_fingerprint: Some(head.content_sha256.clone()) },
    );
    save_state(folder, &state)
}

pub fn sync(serial: &str, launcher_version: &str, strategy: Strategy) -> Result<CloudSyncResult, String> {
    let _guard = CLOUD_LOCK.lock().map_err(|_| "cloud saves are busy".to_string())?;
    let folder = selected_folder().ok_or("choose a cloud save folder first")?;
    let (before, local) = status_with(Some(&folder), serial, launcher_version)?;
    let snapshots = list_meta(&folder, serial)?;
    let tips = heads(&snapshots);

    let (action, message) = match strategy {
        Strategy::UseLocal => {
            upload(&folder, serial, launcher_version, local)?;
            ("uploaded", "This device's saves were added as a new cloud snapshot.".to_string())
        }
        Strategy::UseCloud => {
            if tips.len() != 1 {
                if let Some((path, _, _)) = local {
                    let _ = fs::remove_file(path);
                }
                return Err(if tips.is_empty() {
                    "there is no cloud save to restore".into()
                } else {
                    "the cloud has multiple conflicting save branches; keep this device to resolve them, or inspect the cloud folder".into()
                });
            }
            if let Some((path, _, _)) = local {
                let _ = fs::remove_file(path);
            }
            download(&folder, serial, tips[0])?;
            ("downloaded", "The cloud snapshot was restored after a local backup.".to_string())
        }
        Strategy::Auto if before.conflict => {
            if let Some((path, _, _)) = local {
                let _ = fs::remove_file(path);
            }
            (
                "conflict",
                "Both this device and the cloud changed. Choose which one to keep; neither was overwritten."
                    .to_string(),
            )
        }
        Strategy::Auto if !before.local_has_saves && tips.len() == 1 => {
            if let Some((path, _, _)) = local {
                let _ = fs::remove_file(path);
            }
            download(&folder, serial, tips[0])?;
            ("downloaded", "The cloud snapshot was restored to this device.".to_string())
        }
        Strategy::Auto if before.cloud_changed && tips.len() == 1 => {
            if let Some((path, _, _)) = local {
                let _ = fs::remove_file(path);
            }
            download(&folder, serial, tips[0])?;
            ("downloaded", "A newer cloud snapshot was restored after a local backup.".to_string())
        }
        Strategy::Auto if before.local_changed => {
            upload(&folder, serial, launcher_version, local)?;
            ("uploaded", "The local saves were added to the cloud folder.".to_string())
        }
        Strategy::Auto if tips.is_empty() && before.local_has_saves => {
            upload(&folder, serial, launcher_version, local)?;
            ("uploaded", "The local saves were added to the cloud folder.".to_string())
        }
        Strategy::Auto => {
            if let Some((path, _, _)) = local {
                let _ = fs::remove_file(path);
            }
            ("none", "Cloud saves are already in sync.".to_string())
        }
    };

    Ok(CloudSyncResult { action: action.into(), message, status: status_unlocked(serial, launcher_version)? })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAL: &str = "SCES_509.16";
    const FOLDER: &str = "BESCES-50916RATCHET";

    fn save(planet: u32, bolts: u32) -> Vec<u8> {
        let mut out = vec![0u8; 16];
        let clock = [0x00u8, 0x14, 0x14, 0x23, 0x00, 0x10, 0x10, 0x26];
        for (id, data) in
            [(0i32, planet.to_le_bytes().to_vec()), (1, bolts.to_le_bytes().to_vec()), (4, clock.to_vec())]
        {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&(data.len() as i32).to_le_bytes());
            out.extend_from_slice(&data);
        }
        out.extend_from_slice(&(-1i32).to_le_bytes());
        out
    }

    fn card(home: &Path, bolts: u32) {
        let dir = home.join("memcard").join(PAL).join(FOLDER);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("save0.bin"), save(3, bolts)).unwrap();
        fs::write(dir.join("icon.sys"), b"").unwrap();
    }

    #[test]
    fn a_folder_backend_uploads_then_restores_on_another_device() {
        let provider = tempfile::tempdir().unwrap();
        let first = tempfile::tempdir().unwrap();
        saves::with_data_home(first.path(), || {
            card(first.path(), 111);
            set_folder(Some(provider.path().to_str().unwrap())).unwrap();
            let r = sync(PAL, "test", Strategy::Auto).unwrap();
            assert_eq!(r.action, "uploaded");
            assert_eq!(r.status.snapshots, 1);
            assert_eq!(r.status.state, "synced");
        });

        let second = tempfile::tempdir().unwrap();
        saves::with_data_home(second.path(), || {
            set_folder(Some(provider.path().to_str().unwrap())).unwrap();
            let r = sync(PAL, "test", Strategy::Auto).unwrap();
            assert_eq!(r.action, "downloaded");
            let s = saves::inspect_saves(PAL);
            assert!(s.slots.iter().any(|slot| !slot.is_empty && slot.bolts == Some(111)));
        });
    }

    #[test]
    fn two_changed_devices_report_a_conflict_without_overwriting() {
        let provider = tempfile::tempdir().unwrap();
        let first = tempfile::tempdir().unwrap();
        saves::with_data_home(first.path(), || {
            card(first.path(), 100);
            set_folder(Some(provider.path().to_str().unwrap())).unwrap();
            sync(PAL, "test", Strategy::Auto).unwrap();
        });

        let second = tempfile::tempdir().unwrap();
        saves::with_data_home(second.path(), || {
            set_folder(Some(provider.path().to_str().unwrap())).unwrap();
            sync(PAL, "test", Strategy::Auto).unwrap();
            card(second.path(), 200);
        });

        saves::with_data_home(first.path(), || {
            card(first.path(), 300);
            sync(PAL, "test", Strategy::UseLocal).unwrap();
        });

        saves::with_data_home(second.path(), || {
            let r = sync(PAL, "test", Strategy::Auto).unwrap();
            assert_eq!(r.action, "conflict");
            assert!(r.status.conflict);
            let s = saves::inspect_saves(PAL);
            assert!(s.slots.iter().any(|slot| !slot.is_empty && slot.bolts == Some(200)));

            let resolved = sync(PAL, "test", Strategy::UseLocal).unwrap();
            assert_eq!(resolved.action, "uploaded");
            assert!(!resolved.status.conflict);
            assert_eq!(resolved.status.heads.len(), 1);
        });
    }

    #[test]
    fn selecting_the_same_folder_keeps_the_last_seen_snapshot() {
        let provider = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        saves::with_data_home(home.path(), || {
            card(home.path(), 100);
            let folder = provider.path().to_str().unwrap();
            set_folder(Some(folder)).unwrap();
            assert_eq!(sync(PAL, "test", Strategy::Auto).unwrap().action, "uploaded");

            // Re-opening the folder picker and choosing the same folder must not forget what this
            // device last synced. A later local edit is a normal upload, not a false conflict.
            set_folder(Some(folder)).unwrap();
            card(home.path(), 200);
            let r = sync(PAL, "test", Strategy::Auto).unwrap();
            assert_eq!(r.action, "uploaded");
            assert!(!r.status.conflict);
        });
    }

    #[test]
    fn a_damaged_cloud_object_is_refused_without_touching_the_local_card() {
        let provider = tempfile::tempdir().unwrap();
        let first = tempfile::tempdir().unwrap();
        saves::with_data_home(first.path(), || {
            card(first.path(), 111);
            set_folder(Some(provider.path().to_str().unwrap())).unwrap();
            sync(PAL, "test", Strategy::Auto).unwrap();
        });

        let object_dir = objects_dir(provider.path(), PAL).unwrap();
        let object = fs::read_dir(&object_dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "zip"))
            .unwrap();
        fs::write(&object, b"damaged").unwrap();

        let second = tempfile::tempdir().unwrap();
        saves::with_data_home(second.path(), || {
            set_folder(Some(provider.path().to_str().unwrap())).unwrap();
            let error = sync(PAL, "test", Strategy::Auto).unwrap_err();
            assert!(error.contains("damaged") || error.contains("changed"));
            assert!(!saves::inspect_saves(PAL).slots.iter().any(|slot| !slot.is_empty));
        });
    }
}
