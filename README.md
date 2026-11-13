# Sucky — Cache Cleaner

Standalone Windows desktop app (Rust + egui). Finds common dev caches and gives you one button to purge them, including `rush purge` for Rush.js monorepos.

## Targets

| Target | What it does |
| --- | --- |
| User TEMP folder | Deletes `%TEMP%` contents. Locked/in-use files are skipped. **Unchecked by default.** |
| npm cache | Deletes the npm cache dir (`npm config get cache`, fallback `%LOCALAPPDATA%\npm-cache`) |
| pip cache | Runs `python -m pip cache purge` |
| pnpm store | Runs `pnpm store prune` (only if pnpm on PATH) |
| yarn cache | Runs `yarn cache clean` (only if yarn on PATH) |
| NuGet packages | Deletes `%USERPROFILE%\.nuget\packages` contents. **Unchecked by default.** |
| Rush repos | Add repo folders containing `rush.json`; clean runs `rush purge` there (falls back to deleting `common\temp`). Persisted in `%APPDATA%\rush-button\config.json` |

## Prereqs

1. Rust (GNU toolchain, avoids Visual Studio Build Tools):
   ```powershell
   winget install Rustlang.Rustup
   winget install BrechtSanders.WinLibs.POSIX.UCRT
   rustup default stable-x86_64-pc-windows-gnu
   ```
   MSVC also works if you have VS Build Tools ("Desktop development with C++").
2. Nothing else. No Node, no npm for this app itself.

## Run

```powershell
cd C:\Users\ramen\orca\projects\cache-cleaner
cargo run --release
```

First build downloads crates (eframe etc.) — takes a few minutes.

## Safety

- Only ever touches the known cache roots listed above. Never scans your whole disk.
- `%TEMP%` and NuGet are opt-in checkboxes.
- Confirmation dialog with the selected total size before anything is deleted.
- Files in use (locked by a running app) are skipped and logged, never force-deleted.
