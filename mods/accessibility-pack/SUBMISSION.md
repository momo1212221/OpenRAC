# Submission checklist

This file prepares the mod for a later OpenRAC index submission. **Nothing in this folder submits the mod.**

1. Keep `openrac-mod.json`, source, license and README together.
2. Ensure the standalone build workflow is green on Windows, Linux, macOS and Android.
3. Create the release tag `accessibility-pack-v0.1.0` if publishing from the current multi-mod repository, or `v0.1.0` if this folder has been moved to its own repository.
4. Verify the release contains the merged mod archive and platform archives.
5. Copy `submission/index-entry.json` to `OpenRAC/openrac-mods/index/accessibility-pack.json` in a future submission PR and run the index checker.
6. Wait for maintainer review/approval before treating the mod as published.

Prepared author: **Moritz Wallinger**.
