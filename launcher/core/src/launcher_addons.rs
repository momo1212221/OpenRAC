// SPDX-License-Identifier: GPL-3.0-or-later
//! Independently enabled launcher mods. State is separate from renderer settings and save data.
use crate::differences::{read_switch, settings_file};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
pub const BACKUP_ID: &str = "save-backup-sync";
static LOCK: Mutex<()> = Mutex::new(());
static ACTIVE: Mutex<BTreeMap<String, usize>> = Mutex::new(BTreeMap::new());
fn active_key(home: &Path, game: &str, version: &str) -> String {
    format!("{}:{game}/{version}", home.display())
}
pub struct PreparationGuard {
    key: String,
}
impl Drop for PreparationGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE.lock() {
            if let Some(count) = active.get_mut(&self.key) {
                *count -= 1;
                if *count == 0 {
                    active.remove(&self.key);
                }
            }
        }
    }
}
pub fn begin_preparation(home: &Path, game: &str, version: &str) -> Result<PreparationGuard, String> {
    let _lock = LOCK.lock().map_err(|e| e.to_string())?;
    require(home, crate::graphics_preparation::ID, game, version)?;
    let key = active_key(home, game, version);
    *ACTIVE.lock().map_err(|e| e.to_string())?.entry(key.clone()).or_default() += 1;
    Ok(PreparationGuard { key })
}

#[derive(Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    graphics: BTreeMap<String, bool>,
    backup: Option<bool>,
}
fn file(home: &Path) -> PathBuf {
    home.join("launcher-modules.json")
}
fn read(home: &Path) -> Result<State, String> {
    match std::fs::read_to_string(file(home)) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("Module settings are invalid: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn enabled(home: &Path, id: &str, game: &str, version: &str) -> bool {
    let Ok(state) = read(home) else { return false };
    if id == BACKUP_ID {
        return state.backup.unwrap_or_else(|| {
            std::fs::read_to_string(home.join("cloud-saves.json"))
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .is_some_and(|v| v["folder"].as_str().is_some_and(|s| !s.is_empty()))
        });
    }
    if id != crate::graphics_preparation::ID {
        return false;
    }
    *state.graphics.get(&format!("{game}/{version}")).unwrap_or(&{
        // Preserve explicitly configured preparation from earlier fork installations.
        let text = std::fs::read_to_string(settings_file(home, game, version)).unwrap_or_default();
        ["hardware_auto", "offline_ultra_textures", "cloud_enabled", "ray_tracing"]
            .iter()
            .any(|key| read_switch(&text, key) == Some(true))
    })
}
/// Freeze one-time migration before a user edits settings on the new mod pages.
pub fn initialize(home: &Path, game: &str, version: &str) -> Result<(), String> {
    let _lock = LOCK.lock().map_err(|e| e.to_string())?;
    let mut state = read(home)?;
    let key = format!("{game}/{version}");
    let needs_graphics = !state.graphics.contains_key(&key);
    let needs_backup = state.backup.is_none();
    if !needs_graphics && !needs_backup {
        return Ok(());
    }
    if needs_graphics {
        state.graphics.insert(key, enabled(home, crate::graphics_preparation::ID, game, version));
    }
    if needs_backup {
        state.backup = Some(enabled(home, BACKUP_ID, game, version));
    }
    crate::differences::write_settings(
        &file(home),
        &(serde_json::to_string_pretty(&state).map_err(|e| e.to_string())? + "\n"),
    )
}

pub fn set_enabled(home: &Path, id: &str, game: &str, version: &str, on: bool) -> Result<(), String> {
    let _lock = LOCK.lock().map_err(|e| e.to_string())?;
    if id == crate::graphics_preparation::ID
        && !on
        && ACTIVE.lock().map_err(|e| e.to_string())?.get(&active_key(home, game, version)).copied().unwrap_or(0) > 0
    {
        return Err("Wait for asset preparation to finish before disabling its mod".into());
    }
    let mut state = read(home)?;
    match id {
        BACKUP_ID => state.backup = Some(on),
        crate::graphics_preparation::ID => {
            state.graphics.insert(format!("{game}/{version}"), on);
        }
        _ => return Err("Unknown launcher mod".into()),
    }
    crate::differences::write_settings(
        &file(home),
        &(serde_json::to_string_pretty(&state).map_err(|e| e.to_string())? + "\n"),
    )
}
pub fn require(home: &Path, id: &str, game: &str, version: &str) -> Result<(), String> {
    if enabled(home, id, game, version) {
        Ok(())
    } else {
        Err(format!("Enable {id} in Mods first"))
    }
}
pub fn backup_view(home: &Path, game: &str, version: &str) -> crate::enhanced::EnhancedView {
    let mut view = crate::enhanced::view_all(home, game, version, Default::default());
    view.entry.id = BACKUP_ID.into();
    view.entry.name = "Backup & Cloud Saves".into();
    view.entry.description = "Independent save backups, restore and folder-based cloud sync. Works without any graphics mod. Your cloud app synchronizes the folder; OpenRAC does not sign into a cloud provider.".into();
    view.entry.version = env!("CARGO_PKG_VERSION").into();
    view.entry.enabled = enabled(home, BACKUP_ID, game, version);
    view.entry.state = if view.entry.enabled { "ready" } else { "off" }.into();
    view.entry.settings.clear();
    view.entry.presets.clear();
    view
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modules_and_renderer_are_independent_and_data_is_preserved() {
        let t = tempfile::tempdir().unwrap();
        let h = t.path();
        crate::enhanced::set_enabled(h, "rac1", "pal", true).unwrap();
        assert!(!enabled(h, crate::graphics_preparation::ID, "rac1", "pal"));
        set_enabled(h, crate::graphics_preparation::ID, "rac1", "pal", true).unwrap();
        set_enabled(h, BACKUP_ID, "rac1", "pal", true).unwrap();
        let cache = h.join("probes/pal/keep");
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
        std::fs::write(&cache, "cached").unwrap();
        set_enabled(h, crate::graphics_preparation::ID, "rac1", "pal", false).unwrap();
        assert!(enabled(h, BACKUP_ID, "rac2", "ntsc"));
        assert!(!enabled(h, crate::graphics_preparation::ID, "rac1", "pal"));
        assert!(crate::enhanced::view(h, "rac1", "pal", Default::default()).entry.enabled);
        assert_eq!(std::fs::read_to_string(cache).unwrap(), "cached");
    }
    #[test]
    fn legacy_settings_migrate_without_overriding_an_explicit_off() {
        let t = tempfile::tempdir().unwrap();
        let h = t.path();
        crate::enhanced::set_setting(h, "rac1", "pal", "offline_ultra_textures", &true.into()).unwrap();
        assert!(enabled(h, crate::graphics_preparation::ID, "rac1", "pal"));
        set_enabled(h, crate::graphics_preparation::ID, "rac1", "pal", false).unwrap();
        assert!(!enabled(h, crate::graphics_preparation::ID, "rac1", "pal"));
        assert!(!enabled(h, crate::graphics_preparation::ID, "rac2", "ntsc"));
    }
    #[test]
    fn editing_settings_after_registration_does_not_enable_a_mod() {
        let t = tempfile::tempdir().unwrap();
        let h = t.path();
        initialize(h, "rac1", "pal").unwrap();
        crate::enhanced::set_setting(h, "rac1", "pal", "cloud_enabled", &true.into()).unwrap();
        initialize(h, "rac1", "pal").unwrap();
        assert!(!enabled(h, crate::graphics_preparation::ID, "rac1", "pal"));
    }
    #[test]
    fn active_asset_preparation_cannot_be_disabled_mid_write() {
        let t = tempfile::tempdir().unwrap();
        let h = t.path();
        set_enabled(h, crate::graphics_preparation::ID, "rac1", "pal", true).unwrap();
        let guard = begin_preparation(h, "rac1", "pal").unwrap();
        assert!(set_enabled(h, crate::graphics_preparation::ID, "rac1", "pal", false).is_err());
        drop(guard);
        set_enabled(h, crate::graphics_preparation::ID, "rac1", "pal", false).unwrap();
        assert!(begin_preparation(h, "rac1", "pal").is_err());
    }
    #[test]
    fn corrupt_state_is_never_overwritten_and_fails_closed() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(file(t.path()), "bad").unwrap();
        assert!(!enabled(t.path(), BACKUP_ID, "", ""));
        assert!(set_enabled(t.path(), BACKUP_ID, "", "", true).is_err());
        assert_eq!(std::fs::read_to_string(file(t.path())).unwrap(), "bad");
    }
    #[test]
    fn settings_have_one_owner() {
        let t = tempfile::tempdir().unwrap();
        let h = t.path();
        let renderer = crate::enhanced::view(h, "rac1", "pal", Default::default());
        let prep = crate::graphics_preparation::view(h, "rac1", "pal", Default::default());
        assert!(renderer.entry.settings.iter().all(|s| !crate::graphics_preparation::owns(&s.spec.key)));
        assert!(prep.entry.settings.iter().all(|s| crate::graphics_preparation::owns(&s.spec.key)));
        assert!(prep.entry.settings.iter().any(|s| s.spec.key == "cloud_enabled"));
    }
}
