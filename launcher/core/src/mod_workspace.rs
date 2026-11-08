//! A mod tool page's user-selected working folder.
//!
//! The page never receives the folder's absolute path. The launcher remembers one folder per mod
//! after the player picks it, and every operation below takes a relative path that is kept inside
//! that folder. This is deliberately a file capability, not general filesystem access.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::mod_tools::{self, ToolFile};

/// The most bytes one workspace read or write may move in one call.
pub const FILE_MAX: u64 = 64 << 20;
/// The most files a workspace listing may return.
pub const LIST_MAX: usize = 200_000;
/// Kept beside a mod tool's JSON store, but not a key the page can read or overwrite.
const SELECTED: &str = ".workspace";

fn selected_file(data_home: &Path, id: &str) -> Result<PathBuf, String> {
    Ok(mod_tools::store_dir(data_home, id)?.join(SELECTED))
}

/// The folder the player chose for this mod, when it still exists.
pub fn selected(data_home: &Path, id: &str) -> Result<Option<PathBuf>, String> {
    let file = selected_file(data_home, id)?;
    let text = match fs::read_to_string(&file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", file.display())),
    };
    let path = PathBuf::from(text.trim());
    if path.as_os_str().is_empty() || !path.is_dir() {
        return Ok(None);
    }
    path.canonicalize().map(Some).map_err(|e| format!("{}: {e}", path.display()))
}

/// Remembers a folder the trusted launcher UI just let the player pick.
pub fn select(data_home: &Path, id: &str, path: &Path) -> Result<PathBuf, String> {
    let path = path.canonicalize().map_err(|e| format!("{}: {e}", path.display()))?;
    if !path.is_dir() {
        return Err(format!("{} is not a folder", path.display()));
    }
    let text = path.to_str().ok_or("the selected folder's name is not UTF-8")?;
    let file = selected_file(data_home, id)?;
    let dir = file.parent().expect("the workspace selection has a folder");
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let part = dir.join(".workspace.part");
    fs::write(&part, text).map_err(|e| format!("{}: {e}", part.display()))?;
    fs::rename(&part, &file).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(path)
}

/// Forgets the selected folder. Nothing in the folder is changed.
pub fn clear(data_home: &Path, id: &str) -> Result<(), String> {
    let file = selected_file(data_home, id)?;
    match fs::remove_file(&file) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

fn plain(rel: &str) -> bool {
    !rel.is_empty()
        && !rel.contains('\\')
        && !rel.contains('\0')
        && Path::new(rel).components().all(|c| matches!(c, Component::Normal(_)))
}

/// Every regular file below the selected folder, by relative path. Links and hidden entries are
/// left out, so a selected folder cannot lead the page somewhere else.
pub fn list(root: &Path) -> Result<Vec<ToolFile>, String> {
    let root = root.canonicalize().map_err(|e| format!("{}: {e}", root.display()))?;
    if !root.is_dir() {
        return Err(format!("{} is not a folder", root.display()));
    }
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let dir = root.join(&rel);
        for entry in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else { continue };
            if name.starts_with('.') || kind.is_symlink() {
                continue;
            }
            let child = rel.join(name);
            if kind.is_dir() {
                stack.push(child);
            } else if kind.is_file() {
                if out.len() >= LIST_MAX {
                    return Err(format!("the workspace holds more than {LIST_MAX} files"));
                }
                let size = entry.metadata().map_err(|e| e.to_string())?.len();
                let path = child.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
                out.push(ToolFile { path, size });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// One existing file below the selected folder, at most FILE_MAX bytes.
pub fn read(root: &Path, rel: &str) -> Result<Vec<u8>, String> {
    let path = mod_tools::inside(root, rel)?;
    if !path.is_file() {
        return Err(format!("{rel} is not a file"));
    }
    let size = path.metadata().map_err(|e| e.to_string())?.len();
    if size > FILE_MAX {
        return Err(format!("{rel} is larger than {} MB", FILE_MAX >> 20));
    }
    fs::read(&path).map_err(|e| format!("{rel}: {e}"))
}

/// A destination below the selected folder. Missing parent folders are made one by one; an
/// existing link or non-folder anywhere on the way is refused.
fn destination(root: &Path, rel: &str) -> Result<PathBuf, String> {
    if !plain(rel) {
        return Err(format!("{rel:?} is not a path inside the workspace"));
    }
    let base = root.canonicalize().map_err(|e| format!("{}: {e}", root.display()))?;
    if !base.is_dir() {
        return Err(format!("{} is not a folder", base.display()));
    }
    let rel_path = Path::new(rel);
    let name = rel_path.file_name().ok_or("no file name")?;
    let mut dir = base.clone();
    if let Some(parent) = rel_path.parent() {
        for component in parent.components() {
            let Component::Normal(part) = component else {
                return Err(format!("{rel:?} is not a path inside the workspace"));
            };
            let next = dir.join(part);
            match fs::symlink_metadata(&next) {
                Ok(meta) => {
                    if meta.file_type().is_symlink() || !meta.is_dir() {
                        return Err(format!("{} is not a workspace folder", next.display()));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    fs::create_dir(&next).map_err(|e| format!("{}: {e}", next.display()))?;
                }
                Err(e) => return Err(format!("{}: {e}", next.display())),
            }
            let canonical = next.canonicalize().map_err(|e| format!("{}: {e}", next.display()))?;
            if !canonical.starts_with(&base) {
                return Err(format!("{rel:?} leads out of the workspace"));
            }
            dir = canonical;
        }
    }
    let target = dir.join(name);
    if let Ok(meta) = fs::symlink_metadata(&target) {
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(format!("{} is not a regular workspace file", target.display()));
        }
    }
    Ok(target)
}

/// Writes one file below the selected folder. The write is staged beside it; Unix replaces an
/// existing file atomically, while Windows falls back to removing the old regular file first.
pub fn write(root: &Path, rel: &str, bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() {
        return Err("no bytes to write".into());
    }
    if bytes.len() as u64 > FILE_MAX {
        return Err(format!("the file is larger than {} MB", FILE_MAX >> 20));
    }
    let target = destination(root, rel)?;
    let name = target.file_name().and_then(|n| n.to_str()).ok_or("the file name is not UTF-8")?;
    let part = target.with_file_name(format!(".{name}.openrac-part"));
    if let Ok(meta) = fs::symlink_metadata(&part) {
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(format!("{} is not a regular staging file", part.display()));
        }
        fs::remove_file(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    }
    fs::write(&part, bytes).map_err(|e| format!("{}: {e}", part.display()))?;
    match fs::rename(&part, &target) {
        Ok(()) => Ok(()),
        Err(first) if target.is_file() => {
            fs::remove_file(&target).map_err(|e| format!("{}: {e}", target.display()))?;
            fs::rename(&part, &target).map_err(|e| format!("{}: {e} (after {first})", target.display()))
        }
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(format!("{}: {e}", target.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(name: &str) -> (tempfile::TempDir, tempfile::TempDir) {
        let home = tempfile::Builder::new().prefix(&format!("openrac-mod-workspace-home-{name}-")).tempdir().unwrap();
        let work = tempfile::Builder::new().prefix(&format!("openrac-mod-workspace-work-{name}-")).tempdir().unwrap();
        (home, work)
    }

    #[test]
    fn selection_is_per_mod_and_can_be_forgotten() {
        let (home, work) = dirs("selection");
        let chosen = select(home.path(), "cloud-saves", work.path()).unwrap();
        assert_eq!(selected(home.path(), "cloud-saves").unwrap(), Some(chosen));
        assert_eq!(selected(home.path(), "other-mod").unwrap(), None);
        clear(home.path(), "cloud-saves").unwrap();
        assert_eq!(selected(home.path(), "cloud-saves").unwrap(), None);
    }

    #[test]
    fn writes_lists_and_reads_only_inside_the_workspace() {
        let (_home, work) = dirs("files");
        write(work.path(), "OpenRAC Cloud Saves/v1/card.zip", b"PK backup").unwrap();
        write(work.path(), "outputs/texture.png", b"png").unwrap();
        write(work.path(), "outputs/texture.png", b"new").unwrap();

        let files = list(work.path()).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["OpenRAC Cloud Saves/v1/card.zip", "outputs/texture.png"]);
        assert_eq!(read(work.path(), "outputs/texture.png").unwrap(), b"new");

        for bad in ["../x", "/tmp/x", "a/../x", "a\\x", "./x", ""] {
            assert!(write(work.path(), bad, b"x").is_err(), "{bad:?} was writable");
        }
    }

    #[cfg(unix)]
    #[test]
    fn links_do_not_escape_the_workspace() {
        let (_home, work) = dirs("links");
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.zip"), b"secret").unwrap();
        std::os::unix::fs::symlink(outside.path(), work.path().join("link")).unwrap();
        assert!(read(work.path(), "link/secret.zip").is_err());
        assert!(write(work.path(), "link/new.zip", b"x").is_err());
        assert!(list(work.path()).unwrap().is_empty());
    }
}
