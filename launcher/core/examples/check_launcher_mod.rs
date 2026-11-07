// SPDX-License-Identifier: GPL-3.0-or-later
//! Verify a distributable mod package in an isolated temporary install, without a game run.
use openrac_launcher_core::{launcher_addons as addons, plugins};
use std::path::Path;
fn main() {
    for arg in std::env::args().skip(1) {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("plugins");
        plugins::install(&dir, "rac1", "pal", Path::new(&arg)).unwrap();
        let id = if arg.contains("graphics-preparation") { "graphics-preparation" } else { addons::BACKUP_ID };
        let folder = dir.join(id);
        addons::set_enabled(t.path(), id, "rac1", "pal", true).unwrap();
        assert!(!addons::active(t.path(), id, "rac1", "pal"));
        addons::grant(t.path(), &folder, id, "rac1").unwrap();
        assert!(addons::active(t.path(), id, "rac1", "pal"));
        addons::set_enabled(t.path(), id, "rac1", "pal", false).unwrap();
        assert!(!addons::active(t.path(), id, "rac1", "pal"));
        addons::set_enabled(t.path(), id, "rac1", "pal", true).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
        assert!(!addons::active(t.path(), id, "rac1", "pal"));
        println!("Verified package install, permission, toggle and uninstall: {arg}");
    }
}
