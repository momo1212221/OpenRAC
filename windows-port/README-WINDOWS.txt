OpenRAC Ultra - Windows transfer package
========================================

This package is a Windows port of the exact renderer source used by the current Mac test build.

Windows target
--------------
- Windows 10/11 x64
- NVIDIA RTX 20-series or RTX 30-series supported
- Vulkan is the default renderer
- OpenRAC exports NvOptimusEnablement=1, so NVIDIA is preferred on hybrid-GPU systems
- Runtime texture upscaling stays disabled; the included Ultra4K pack is pre-rendered
- The pre-rendered probe-lighting cache is copied with the package

Install
-------
1. Copy this USB package to a local SSD if possible.
2. Run WindowsPort\Install-Ultra-Data.ps1 in PowerShell.
3. Install the .exe or .msi from WindowsInstaller.
4. Start OpenRAC Launcher.
5. If OpenRAC asks for a game/install folder, select C:\OpenRAC (or the folder you chose in the script).
6. Keep Renderer = Vulkan.

Prepared graphics copied from the Mac
-------------------------------------
- Ultra4K texture cache
- Remastered texture pack
- PAL exported levels
- Probe/GI lighting cache
- Current rac1-pal.ini graphics settings
- Extracted PAL disc data

Notes
-----
- macOS .dylib plugins are intentionally not copied. Install Windows-compatible mods/plugins again from the Windows launcher.
- The lighting and texture assets are platform-neutral.
- The source on the Mac was not modified for this Windows package. Windows changes were made in a separate staging copy.
- A source snapshot / Git branch is included so the Windows build is reproducible.
