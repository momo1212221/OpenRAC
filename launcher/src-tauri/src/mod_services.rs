// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed, consented services for independently installed mod pages. No shell or arbitrary paths.
use crate::*;
use openrac_launcher_core::{graphics_preparation as prep, launcher_addons as addons};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::LazyLock;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
static BAKES: LazyLock<Mutex<HashMap<String, u32>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
fn installed(state: &State<'_, AppState>, game: &str, id: &str) -> Result<PathBuf, String> {
    if !matches!(id, prep::ID | addons::BACKUP_ID) || !matches!(game, "rac1" | "rac2" | "rac3" | "rac4") {
        return Err("Unknown mod service".into());
    }
    let root = plugins_dir(state, game)?.canonicalize().map_err(|e| e.to_string())?;
    let folder = root.join(id).canonicalize().map_err(|e| e.to_string())?;
    if !folder.starts_with(&root) {
        return Err("Mod is outside its installation folder".into());
    }
    addons::package_service(&folder, id)?;
    Ok(folder)
}
fn value<T: Serialize>(result: Result<T, String>) -> Result<Value, String> {
    serde_json::to_value(result?).map_err(|e| e.to_string())
}
async fn confirm_service(app: &AppHandle, message: String) -> Result<(), String> {
    let dialog = app.dialog().message(message).title("OpenRAC mod permission").buttons(MessageDialogButtons::OkCancel);
    let yes = tauri::async_runtime::spawn_blocking(move || dialog.blocking_show()).await.map_err(|e| e.to_string())?;
    if yes {
        Ok(())
    } else {
        Err("Cancelled".into())
    }
}
#[tauri::command]
pub fn mod_service_permission(state: State<'_, AppState>, game: String, id: String) -> Result<Value, String> {
    if id.contains('/') || id.contains('\\') || id == ".." {
        return Err("Invalid mod id".into());
    }
    let manifest = plugins::read_manifest(&plugins_dir(&state, &game)?.join(&id))?;
    if manifest.tool.as_ref().and_then(|t| t.service.as_ref()).is_none() {
        return Ok(Value::Null);
    }
    installed(&state, &game, &id)?;
    Ok(json!({"service":id,"granted":addons::granted(&openrac_launcher_core::saves::data_home(),&id,&game)}))
}
#[tauri::command]
pub async fn mod_service_grant(
    app: AppHandle,
    state: State<'_, AppState>,
    game: String,
    id: String,
) -> Result<(), String> {
    installed(&state, &game, &id)?;
    let scope = if id == prep::ID {
        "prepare and remove generated lighting/GI, water, cloud, texture and mesh caches; import and enable the texture pack this mod just generated; configure preparation and explicitly select Vulkan/Enhanced graphics when you choose an Enhanced preset"
    } else {
        "create/restore local save backups and synchronize saves with a folder you choose; the enable switch applies to all installed games"
    };
    confirm_service(&app,format!("Allow the installed {id} mod to {scope}? Destructive actions still ask separately. Uninstalling the mod disables its hooks.")).await?;
    let folder = installed(&state, &game, &id)?;
    addons::grant(&openrac_launcher_core::saves::data_home(), &folder, &id, &game)
}
#[tauri::command]
pub async fn mod_service(
    app: AppHandle,
    state: State<'_, AppState>,
    game: String,
    version: String,
    id: String,
    operation: String,
    payload: Value,
) -> Result<Value, String> {
    let folder = installed(&state, &game, &id)?;
    let home = openrac_launcher_core::saves::data_home();
    if !addons::granted(&home, &id, &game) {
        return Err("Open the mod and grant its service permission first".into());
    }
    let config = state.config.lock().unwrap().clone();
    let key = format!("{game}/{version}");
    let catalog = openrac_launcher_core::catalog::load(config.root()?)?;
    let serial = catalog.version(&key).ok_or("Unknown game version")?.serial.clone();
    if operation == "status" {
        return if id == prep::ID {
            let bake = prep::version_bake_status(&config, &key);
            let owned = BAKES
                .lock()
                .map_err(|e| e.to_string())?
                .get(&key)
                .copied()
                .filter(|n| state.jobs.running().contains(n));
            Ok(
                json!({"enabled":addons::enabled(&home,&id,&game,&version),"view":prep::view(&home,&game,&version,bake),"job":owned,"log":prep::log_tail(&config,&key).unwrap_or_default()}),
            )
        } else {
            Ok(
                json!({"enabled":addons::enabled(&home,&id,&game,&version),"saves":inspect_saves(serial.clone()),"cloud":cloud_saves_status(app,serial)?,"config":cloud_saves_config()}),
            )
        };
    }
    if operation == "enable" {
        let on = payload["enabled"].as_bool().ok_or("enabled must be boolean")?;
        launcher_addons_set_enabled(state, game, version, id, on).await?;
        return Ok(Value::Null);
    }
    addons::require(&home, &id, &game, &version)?;
    // Native confirmation cannot be forged or skipped by a mod page's Javascript.
    if matches!(operation.as_str(), "bake.clear" | "textures.clear" | "backup.restore")
        || (operation == "cloud.sync" && payload["strategy"].as_str().is_some_and(|s| s != "auto"))
    {
        confirm_service(
            &app,
            format!(
                "{id} requests {operation} for {key}. This can replace saves or delete generated caches. Continue?"
            ),
        )
        .await?;
        installed(&state, &game, &id)?;
        addons::require(&home, &id, &game, &version)?;
    }
    if id == prep::ID {
        match operation.as_str() {
            "setting" => value(
                launcher_addons_set_setting(
                    state,
                    game,
                    version,
                    id,
                    payload["key"].as_str().ok_or("Missing setting key")?.into(),
                    payload["value"].clone(),
                )
                .await,
            ),
            "renderer.enable" => {
                if config.renderer != "vulkan" {
                    let file = state.config_file.as_deref().ok_or("no per-user config folder")?;
                    let mut next = config.clone();
                    next.renderer = "vulkan".into();
                    config::save(file, &next)?;
                    *state.config.lock().unwrap() = next;
                    launcher_log::line("graphics preset: renderer set to Vulkan");
                }
                value(enhanced_set_enabled(state, game, version, true).await)
            }
            "bake.start" => {
                if prep::version_bake_status(&config, &key).busy {
                    return Err("Lighting preparation is already running".into());
                }
                let job =
                    run_action(app, state, Scope::Version(key.clone()), prep::BAKE_ACTION.into(), None, None, None)?;
                BAKES.lock().map_err(|e| e.to_string())?.insert(key, job.id);
                value(Ok(job))
            }
            "bake.stop" => {
                let job = BAKES
                    .lock()
                    .map_err(|e| e.to_string())?
                    .get(&key)
                    .copied()
                    .ok_or("No lighting job was started by this mod session")?;
                state.jobs.cancel(job)?;
                Ok(Value::Null)
            }
            "bake.clear" => value(enhanced_delete_bakes(state, game, version).await),
            "textures.prepare" => value(texture_packs_prepare_ultra(state, game, version).await),
            "textures.sources.prepare" => {
                let data = game_data_dir(&state, &game)?;
                let home = openrac_launcher_core::saves::data_home();
                let g = game.clone();
                let v = version.clone();
                let seeded = tauri::async_runtime::spawn_blocking(move || {
                    openrac_launcher_core::texture_packs::seed_dump_from_levels(&home, &data, &g, &v)
                })
                .await
                .map_err(|e| e.to_string())??;
                value(Ok(seeded))
            }
            "textures.clear" => value(graphics_delete_texture_cache(state, game, version)),
            "ai.textures" => {
                let result = ai_texture_bake(
                    state,
                    game.clone(),
                    version.clone(),
                    serde_json::from_value(payload).map_err(|e| format!("AI options: {e}"))?,
                )
                .await?;
                openrac_launcher_core::texture_packs::set_enabled(&home, &game, &version, true)?;
                value(Ok(result))
            }
            "texture.install" => {
                let source = crate::mod_tool::last_saved(&id)
                    .ok_or("No completed texture pack from this mod session is available to install")?;
                if !source
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
                {
                    return Err("The most recent completed save is not a texture-pack ZIP".into());
                }
                if !source.is_file() {
                    return Err("The generated texture-pack ZIP is no longer available".into());
                }
                let data = game_data_dir(&state, &game)?;
                let g = game.clone();
                let v = version.clone();
                let source_for_import = source.clone();
                let imported = tauri::async_runtime::spawn_blocking(move || {
                    openrac_launcher_core::texture_packs::import(
                        &data,
                        &g,
                        &v,
                        &source_for_import,
                        openrac_launcher_core::texture_packs::ImportMode::Merge,
                    )
                })
                .await
                .map_err(|e| e.to_string())??;
                openrac_launcher_core::texture_packs::set_enabled(&home, &game, &version, true)?;
                launcher_log::line(&format!(
                    "{game}/{version}: imported and enabled {} generated texture(s) from {}",
                    imported.0,
                    source.display()
                ));
                Ok(json!({
                    "installed": true,
                    "textures": imported.0,
                    "source": openrac_launcher_core::reports::tidy(
                        &source.to_string_lossy(),
                        openrac_launcher_core::reports::home().as_deref(),
                    ),
                }))
            }
            "ai.meshes" => {
                let options = if payload["options"].is_object() {
                    payload["options"].clone()
                } else {
                    let profile = match payload["mode"].as_str().unwrap_or("enhanced") {
                        "faithful" => "enhanced",
                        "balanced" => "high",
                        _ => "modern",
                    };
                    json!({"profile":profile,"scope":"all"})
                };
                let root = config.root()?.to_path_buf();
                let python = config
                    .python
                    .clone()
                    .or_else(|| {
                        let bundled = if cfg!(windows) {
                            root.join("python/python.exe")
                        } else {
                            root.join("python/bin/python3")
                        };
                        bundled.is_file().then_some(bundled)
                    })
                    .ok_or("Python is not configured; reinstall or select the bundled Python in Settings")?;
                let helper = folder.join("helper/openrac_graphics_helper.py");
                let data = game_data_dir(&state, &game)?;
                let material_pack =
                    openrac_launcher_core::texture_packs::folder(&home, &data, &game, &version, false, "")?;
                let _preparation = addons::begin_preparation(&home, &game, &version)?;
                let g = game.clone();
                let v = version.clone();
                let result = tauri::async_runtime::spawn_blocking(move || {
                    crate::asset_ai::bake_with_graphics_helper(crate::asset_ai::GraphicsHelperBake {
                        python: &python,
                        openrac_root: &root,
                        helper: &helper,
                        data: &data,
                        game: &g,
                        version: &v,
                        material_pack: Some(&material_pack),
                        options,
                    })
                })
                .await
                .map_err(|e| e.to_string())??;
                launcher_log::line(&format!(
                    "{game}/{version}: geometry preparation activated {} render mesh(es) at {}",
                    result.processed, result.output
                ));
                value(Ok(result))
            }
            _ => Err("Operation is not available to the graphics preparation mod".into()),
        }
    } else {
        match operation.as_str() {
            "backup.create" => value(backup_saves(serial, payload["note"].as_str().map(str::to_owned))),
            "backup.restore" => {
                value(restore_backup(app, serial, payload["name"].as_str().ok_or("Missing backup name")?.into()).await)
            }
            "cloud.choose" => {
                #[cfg(not(target_os = "android"))]
                {
                    let dialog = app.dialog().file().set_title("Choose the mod's cloud saves folder");
                    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder())
                        .await
                        .map_err(|e| e.to_string())?;
                    let Some(picked) = picked else { return Ok(Value::Null) };
                    value(cloud_saves_set_folder(Some(
                        picked.into_path().map_err(|e| e.to_string())?.to_string_lossy().into_owned(),
                    )))
                }
                #[cfg(target_os = "android")]
                {
                    Err("Folder cloud sync requires the desktop launcher".into())
                }
            }
            "cloud.disconnect" => value(cloud_saves_set_folder(None)),
            "cloud.sync" => {
                value(cloud_saves_sync(app, serial, Some(payload["strategy"].as_str().unwrap_or("auto").into())).await)
            }
            _ => Err("Operation is not available to the backup mod".into()),
        }
    }
}
